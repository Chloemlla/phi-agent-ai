// Safe Markdown renderer. Every block is built as Solid JSX; raw HTML in the
// model output is never injected. The parser is intentionally small and
// streaming-friendly: an unterminated fence keeps rendering as code. Fenced
// `html` blocks and top-level HTML blocks become inline interface blocks,
// rebuilt from an allowlist by `RichHtml`.

import { For, Show, createSignal, onCleanup, type JSX } from "solid-js";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../i18n";
import { IconCheck, IconCopy } from "./Icons";
import { RichHtml } from "./RichHtml";
import {
  parseBlocks,
  parseInline,
  type BlockNode,
  type InlineNode,
} from "./markdown/parse";

function openExternal(url: string, onOpenUrl?: (url: string) => void) {
  if (onOpenUrl) {
    onOpenUrl(url);
    return;
  }
  void openUrl(url).catch(() => undefined);
}

async function writeClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export function CopyButton(props: { text: string; class?: string }) {
  const [copied, setCopied] = createSignal(false);
  let timer: number | undefined;

  onCleanup(() => {
    if (timer) window.clearTimeout(timer);
  });

  async function copy(event: MouseEvent) {
    event.stopPropagation();
    if (!(await writeClipboard(props.text))) return;
    setCopied(true);
    if (timer) window.clearTimeout(timer);
    timer = window.setTimeout(() => setCopied(false), 1500);
  }

  return (
    <button
      type="button"
      classList={{ "copy-btn": true, [props.class ?? ""]: !!props.class }}
      onClick={(event) => void copy(event)}
    >
      <Show when={copied()} fallback={<IconCopy size={12} />}>
        <IconCheck size={12} />
      </Show>
      {copied() ? t("Copied") : t("Copy")}
    </button>
  );
}

// ─────────────────────────── inline rendering ───────────────────────────

type InlineContext = { onOpenUrl?: (url: string) => void };

function renderInline(nodes: InlineNode[], context: InlineContext): JSX.Element[] {
  return nodes.map((node) => {
    switch (node.kind) {
      case "text":
        return node.value;
      case "code":
        return <code class="md-inline-code">{node.value}</code>;
      case "link":
        return (
          <a
            class="md-link"
            href={node.url}
            title={node.url}
            onClick={(event) => {
              event.preventDefault();
              openExternal(node.url, context.onOpenUrl);
            }}
          >
            {renderInline(node.label, context)}
          </a>
        );
      case "strong":
        return <strong>{renderInline(node.children, context)}</strong>;
      case "del":
        return <del>{renderInline(node.children, context)}</del>;
      case "em":
        return <em>{renderInline(node.children, context)}</em>;
    }
  });
}

function Inline(props: { text: string } & InlineContext) {
  return <>{renderInline(parseInline(props.text), { onOpenUrl: props.onOpenUrl })}</>;
}

// ─────────────────────────── rendering ───────────────────────────

function CodeBlock(props: { lang: string; code: string }) {
  return (
    <div class="md-code">
      <div class="md-code-head">
        <span class="md-code-lang">{props.lang || t("Code")}</span>
        <CopyButton text={props.code} class="md-code-copy" />
      </div>
      <pre>
        <code>{props.code}</code>
      </pre>
    </div>
  );
}

function ListItems(props: {
  items: string[][];
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
}) {
  return (
    <For each={props.items}>
      {(item) => (
        <li>
          <Show
            when={item.length > 1}
            fallback={<Inline text={item[0] ?? ""} />}
          >
            <BlockList
              blocks={parseBlocks(item)}
              onOpenUrl={props.onOpenUrl}
              onSend={props.onSend}
            />
          </Show>
        </li>
      )}
    </For>
  );
}

function BlockView(props: {
  block: BlockNode;
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
}) {
  const block = props.block;

  switch (block.kind) {
    case "paragraph":
      return (
        <p>
          <For each={block.lines}>
            {(line, position) => (
              <>
                <Show when={position() > 0}>
                  <br />
                </Show>
                <Inline text={line} onOpenUrl={props.onOpenUrl} />
              </>
            )}
          </For>
        </p>
      );
    case "heading": {
      const level = block.level;
      const content = <Inline text={block.text} onOpenUrl={props.onOpenUrl} />;
      switch (level) {
        case 1:
          return <h1>{content}</h1>;
        case 2:
          return <h2>{content}</h2>;
        case 3:
          return <h3>{content}</h3>;
        case 4:
          return <h4>{content}</h4>;
        case 5:
          return <h5>{content}</h5>;
        default:
          return <h6>{content}</h6>;
      }
    }
    case "code":
      return <CodeBlock lang={block.lang} code={block.code} />;
    case "html":
      return (
        <RichHtml
          html={block.html}
          onOpenUrl={props.onOpenUrl}
          onSend={props.onSend}
        />
      );
    case "quote":
      return (
        <blockquote>
          <BlockList
            blocks={parseBlocks(block.lines)}
            onOpenUrl={props.onOpenUrl}
            onSend={props.onSend}
          />
        </blockquote>
      );
    case "list":
      return (
        <Show
          when={block.ordered}
          fallback={
            <ul class="md-list">
              <ListItems
                items={block.items}
                onOpenUrl={props.onOpenUrl}
                onSend={props.onSend}
              />
            </ul>
          }
        >
          <ol class="md-list" start={block.start}>
            <ListItems
              items={block.items}
              onOpenUrl={props.onOpenUrl}
              onSend={props.onSend}
            />
          </ol>
        </Show>
      );
    case "rule":
      return <hr />;
    case "table":
      return (
        <div class="md-table-wrap">
          <table>
            <thead>
              <tr>
                <For each={block.header}>
                  {(cell) => (
                    <th>
                      <Inline text={cell} onOpenUrl={props.onOpenUrl} />
                    </th>
                  )}
                </For>
              </tr>
            </thead>
            <tbody>
              <For each={block.rows}>
                {(row) => (
                  <tr>
                    <For each={row}>
                      {(cell) => (
                        <td>
                          <Inline text={cell} onOpenUrl={props.onOpenUrl} />
                        </td>
                      )}
                    </For>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      );
    default:
      return null;
  }
}

function BlockList(props: {
  blocks: BlockNode[];
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
}) {
  return (
    <For each={props.blocks}>
      {(block) => (
        <BlockView
          block={block}
          onOpenUrl={props.onOpenUrl}
          onSend={props.onSend}
        />
      )}
    </For>
  );
}

export function Markdown(props: {
  text: string;
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
}) {
  const blocks = () =>
    parseBlocks(props.text.replace(/\r\n?/g, "\n").split("\n"));
  return (
    <div class="md">
      <BlockList
        blocks={blocks()}
        onOpenUrl={props.onOpenUrl}
        onSend={props.onSend}
      />
    </div>
  );
}
