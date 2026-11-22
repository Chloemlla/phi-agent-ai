//! `--help` output for the headless shell.

pub(crate) fn print_help() {
    eprintln!(
        r#"phi-headless — local-first agent node without a UI

Usage:
  phi-headless [global options] <command> [options]

Commands:
  run --message TEXT [--session ID] [--provider ID]
      [--model ID] [--thinking off|low|medium|high|xhigh|max]
      [--approval deny|allow|prompt] [--no-stream] [--title TEXT]
  sessions list
  sessions show <id>
  sessions delete <id>
  agents list <session-id>
  agents show <agent-id>
  image configure --provider openai_images|gemini_images --base-url URL
      --model MODEL [--api-key REF] [--size S] [--quality Q] [--format F]
      [--max-per-turn N] [--max-bytes N] [--grant-egress] [--disabled]
  image show
  image generate --prompt TEXT [--session ID] [--size S] [--count N] [--out PATH]
  image list [--session ID]
  image delete <asset-id>
  decision configure --endpoint URL --model MODEL [--api-key REF]
      [--timeout-ms N] [--triage] [--triage-threshold F] [--triage-tool TOOL]...
      [--compaction] [--compaction-threshold F] [--compaction-min-chars N]
      [--grant-egress] [--disabled]
  decision show
  decision test --state TEXT
  roles list
  roles set <role> --model MODEL [--provider ID] [--thinking LEVEL] [--grant-egress]
  roles clear <role>
  approve <id> --allow|--deny [--note TEXT]
  cancel <run-id>
  status <run-id>
  bootstrap
  providers list
  providers show <id>
  providers add --name NAME --protocol P --base-url URL --model MODEL
      [--api-key env:NAME|literal:VALUE|VALUE] [--auth x_api_key|bearer|none]
      [--max-output-tokens N]
      [--temperature N] [--models a,b] [--model-map role=model]
      [--icon NAME] [--icon-color #hex] [--website URL] [--notes TEXT]
      [--sort-index N] [--activate] [--disabled]
  providers update <id> [same flags]
  providers remove <id>
  providers use <id>
  providers test [id]
  providers models [id]
  memory list [--text TEXT] [--scope user|project|agent|run] [--state STATE]
      [--project ID] [--run ID] [--limit N] [--offset N]
  memory search QUERY [--scope SCOPE] [--limit N] [--min-score N]
      [--include-inactive]
  memory show <id>
  memory add --fact TEXT [--scope user|project|agent]
      [--source user|system|tool|model|external] [--confidence N]
      [--valid-from RFC3339] [--valid-to RFC3339] [--episode TEXT]
      [--episode-source SOURCE] [--episode-label TEXT] [--reason TEXT]
  memory invalidate <id> --reason TEXT
  memory correct <id> --fact TEXT --reason TEXT
  memory supersede <id> --fact TEXT --scope user|project|agent --reason TEXT
      [--source TRUST] [--confidence N] [--valid-from T] [--valid-to T]
  memory forget <id>... | --id ID | --scope SCOPE | --project ID | --run ID | --all
      [--reason TEXT]
  memory maintain [--project ID] [--dry-run|--apply]
  memory stats

Global options:
  --data-dir DIR    default $PHI_DATA_DIR or ./.phi
  --workspace DIR   default $PHI_WORKSPACE or <data-dir>/workspace
  --lang LANG       en or zh-CN (default $PHI_LANG, then saved setting)
  --quiet           suppress per-event JSON lines on stdout
  --json            accepted; stdout is already JSONL

stdout: one JSON object per line (events, model deltas, final result)
stderr: logs and human summaries
exit:   0 success, 3 aborted, 4 failed, 5 needs reconciliation, 2 usage/config error"#
    );
}
