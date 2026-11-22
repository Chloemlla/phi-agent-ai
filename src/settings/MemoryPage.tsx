// Memory page: inspect, search, add and permanently delete stored facts.
// Every operation goes through the node's memory commands; the page never
// touches the database directly.

import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import {
  addMemory as addMemoryCommand,
  forgetMemories,
  getMemoryStats,
  listMemories,
  searchMemories,
} from "../bridge";
import { t, tArgs, lang } from "../i18n";
import { errorText } from "../errors";
import { IconCheck, IconSearch, IconTrash } from "../components/Icons";
import type { MemoryRecord, MemoryStats } from "../types";

function scopeLabel(scope: MemoryRecord["scope"]): string {
  switch (scope) {
    case "user":
      return t("User");
    case "project":
      return t("Project");
    case "agent":
      return t("Agent");
    default:
      return t("Run");
  }
}

function trustLabel(trust: MemoryRecord["sourceTrust"]): string {
  switch (trust) {
    case "user":
      return t("From user");
    case "system":
      return t("From system");
    case "tool":
      return t("From tool");
    case "model":
      return t("From model");
    default:
      return t("From external");
  }
}

function stateLabel(state: MemoryRecord["state"]): string {
  switch (state) {
    case "active":
      return t("State active");
    case "invalidated":
      return t("State invalidated");
    default:
      return t("State superseded");
  }
}

const SEARCH_LIMIT = 50;

function formatTime(raw: string): string {
  const date = new Date(raw);
  return Number.isNaN(date.getTime())
    ? raw
    : date.toLocaleString(lang() === "zh-CN" ? "zh-CN" : "en-US");
}

export function MemoryPage(props: { onRefresh: () => Promise<void> }) {
  const [memories, setMemories] = createSignal<MemoryRecord[]>([]);
  const [stats, setStats] = createSignal<MemoryStats | null>(null);
  const [query, setQuery] = createSignal("");
  const [filtered, setFiltered] = createSignal(false);
  const [truncated, setTruncated] = createSignal(false);
  const [fact, setFact] = createSignal("");
  const [scope, setScope] = createSignal<"user" | "project">("project");
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const [pendingDelete, setPendingDelete] = createSignal<string | null>(null);
  const [armedAt, setArmedAt] = createSignal(0);
  let deleteTimer: ReturnType<typeof setTimeout> | undefined;

  function disarmDelete() {
    if (deleteTimer !== undefined) {
      clearTimeout(deleteTimer);
      deleteTimer = undefined;
    }
    setPendingDelete(null);
  }

  function armDelete(id: string) {
    if (deleteTimer !== undefined) clearTimeout(deleteTimer);
    setPendingDelete(id);
    setArmedAt(Date.now());
    deleteTimer = setTimeout(() => {
      setPendingDelete(null);
      deleteTimer = undefined;
    }, 5000);
  }

  onCleanup(() => {
    if (deleteTimer !== undefined) clearTimeout(deleteTimer);
  });

  async function loadList() {
    // The search box drives BM25 search explicitly; loading the list must not
    // silently apply a substring filter from an unsubmitted query.
    const list = await listMemories(null);
    setMemories(list);
  }

  async function loadStats() {
    setStats(await getMemoryStats());
  }

  async function load() {
    await Promise.all([loadList(), loadStats()]);
  }

  async function runSearch(text: string) {
    const hits = await searchMemories(text, SEARCH_LIMIT);
    setMemories(hits.map((hit) => hit.record));
    setFiltered(true);
    setTruncated(hits.length >= SEARCH_LIMIT);
  }

  /// Reload whatever the page is currently showing: BM25 results stay BM25
  /// results, the unfiltered list stays a list.
  async function reload() {
    if (filtered() && query().trim()) {
      await runSearch(query().trim());
    } else {
      setFiltered(false);
      await loadList();
    }
    await loadStats();
  }

  onMount(async () => {
    try {
      await load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  });

  async function search(event: Event) {
    event.preventDefault();
    disarmDelete();
    setTruncated(false);
    const text = query().trim();
    if (!text) {
      setFiltered(false);
      await refresh();
      return;
    }
    setBusy(true);
    setError("");
    try {
      await runSearch(text);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  async function clearSearch() {
    disarmDelete();
    setQuery("");
    setFiltered(false);
    await refresh();
  }

  async function refresh() {
    try {
      setTruncated(false);
      await load();
    } catch (e) {
      setError(errorText(e));
    }
  }

  async function addMemory(event: Event) {
    event.preventDefault();
    disarmDelete();
    const text = fact().trim();
    if (!text || busy()) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const outcome = await addMemoryCommand(text, scope());
      if (outcome.status === "admitted") {
        setNotice(t("Memory saved."));
      } else if (outcome.status === "duplicate") {
        setNotice(t("This fact is already stored."));
      } else {
        setError(outcome.message);
      }
      if (outcome.status !== "rejected") setFact("");
      await reload();
      await props.onRefresh();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  async function forget(memory: MemoryRecord) {
    if (pendingDelete() !== memory.id) {
      armDelete(memory.id);
      setError("");
      setNotice(t("Click delete again to confirm permanent deletion."));
      return;
    }
    if (Date.now() - armedAt() < 400) {
      return; // An accidental double-click must not confirm.
    }
    disarmDelete();
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const report = await forgetMemories([memory.id]);
      setNotice(
        tArgs("Deleted {records} fact(s) and {episodes} source excerpt(s).", {
          records: report.records,
          episodes: report.episodes,
        }),
      );
      await reload();
      await props.onRefresh();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <div class="settings-page-head">
        <div>
          <h1>{t("Memory")}</h1>
          <p class="settings-subtitle">
            {t(
              "Facts the agent remembers across runs, stored in the local data directory. Deleting a fact erases it and its stored source excerpt from memory; past conversations are not rewritten.",
            )}
          </p>
        </div>
      </div>

      <Show when={stats()}>
        {(summary) => (
          <p class="hint">
            {tArgs("{total} facts · {active} active · {episodes} source excerpts", {
              total: summary().total,
              active: summary().active,
              episodes: summary().episodes,
            })}
          </p>
        )}
      </Show>

      <section class="settings-section">
        <form class="settings-actions memory-form" onSubmit={addMemory}>
          <input
            class="settings-input"
            value={fact()}
            onInput={(event) => setFact(event.currentTarget.value)}
            placeholder={t("Add a stable fact the agent should remember")}
            aria-label={t("Add a stable fact the agent should remember")}
            maxLength={2000}
          />
          <select
            class="settings-input memory-scope"
            value={scope()}
            aria-label={t("Scope")}
            onChange={(event) => setScope(event.currentTarget.value as "user" | "project")}
          >
            <option value="project">{t("Project")}</option>
            <option value="user">{t("User")}</option>
          </select>
          <button class="btn primary" type="submit" disabled={busy() || !fact().trim()}>
            {t("Add")}
          </button>
        </form>

        <form class="settings-actions memory-form" onSubmit={search}>
          <input
            class="settings-input"
            value={query()}
            onInput={(event) => setQuery(event.currentTarget.value)}
            placeholder={t("Search memories")}
            aria-label={t("Search memories")}
          />
          <button class="btn" type="submit" disabled={busy()}>
            <IconSearch size={14} />
            {t("Search")}
          </button>
          <Show when={filtered() || query()}>
            <button class="btn" type="button" onClick={clearSearch} disabled={busy()}>
              {t("Clear")}
            </button>
          </Show>
        </form>
      </section>

      <Show when={error()}>
        <div class="banner danger" role="alert">{error()}</div>
      </Show>
      <Show when={notice()}>
        <div class="settings-note" role="status">
          <IconCheck size={14} />
          {notice()}
        </div>
      </Show>

      <Show when={filtered() && truncated()}>
        <p class="hint">{t("Showing up to 50 matches.")}</p>
      </Show>

      <Show when={loading()}>
        <p class="hint">{t("Loading…")}</p>
      </Show>
      <Show when={!loading() && !error() && memories().length === 0}>
        <p class="hint">
          {filtered()
            ? t("No memories match this search.")
            : t("No memories yet. The agent can save facts with its memory tool.")}
        </p>
      </Show>

      <ul class="memory-list">
        <For each={memories()}>
          {(memory) => (
            <li classList={{ "memory-item": true, inactive: memory.state !== "active" }}>
              <div class="memory-fact">{memory.fact}</div>
              <div class="memory-meta">
                <span class="memory-badge scope">{scopeLabel(memory.scope)}</span>
                <span class="memory-badge">{trustLabel(memory.sourceTrust)}</span>
                <Show when={memory.state !== "active"}>
                  <span class="memory-badge muted">{stateLabel(memory.state)}</span>
                </Show>
                <Show when={memory.redacted}>
                  <span class="memory-badge warn">{t("Redacted")}</span>
                </Show>
                <span>
                  {t("Confidence")} {Math.round(memory.confidence * 100)}%
                </span>
                <span>{formatTime(memory.recordedAt)}</span>
              </div>
              <button
                classList={{
                  "memory-forget": true,
                  confirm: pendingDelete() === memory.id,
                }}
                type="button"
                title={pendingDelete() === memory.id ? t("Confirm delete") : t("Delete")}
                aria-label={pendingDelete() === memory.id ? t("Confirm delete") : t("Delete")}
                disabled={busy()}
                onClick={() => forget(memory)}
              >
                <Show
                  when={pendingDelete() === memory.id}
                  fallback={<IconTrash size={14} />}
                >
                  <IconCheck size={14} />
                </Show>
              </button>
            </li>
          )}
        </For>
      </ul>
    </div>
  );
}
