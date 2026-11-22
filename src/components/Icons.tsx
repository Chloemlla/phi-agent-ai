// UI icons come from Lucide (stroke icons on a 24 viewBox). The brand mark is
// the only custom glyph: φ, per the visual spec.

export {
  ArrowLeft as IconArrowLeft,
  ArrowUp as IconArrowUp,
  Brain as IconBrain,
  Check as IconCheck,
  CheckCircle2 as IconCheckCircle,
  ChevronDown as IconChevronDown,
  ChevronLeft as IconChevronLeft,
  ChevronRight as IconChevron,
  CircleAlert as IconCircleAlert,
  Clock as IconClock,
  Copy as IconCopy,
  Ellipsis as IconEllipsis,
  Eye as IconEye,
  EyeOff as IconEyeOff,
  FolderOpen as IconFolderOpen,
  Hash as IconHash,
  Image as IconImage,
  ImageOff as IconImageOff,
  Info as IconInfo,
  Loader2 as IconSpinner,
  Menu as IconMenu,
  MessageSquare as IconMessageSquare,
  PanelLeft as IconPanel,
  Pencil as IconPencil,
  Plus as IconPlus,
  RefreshCw as IconRefresh,
  Search as IconSearch,
  Server as IconServer,
  Settings as IconSettings,
  Square as IconStop,
  Trash2 as IconTrash,
  TriangleAlert as IconAlert,
  Wrench as IconWrench,
  X as IconClose,
  XCircle as IconXCircle,
} from "lucide-solid";

export function IconPhi(props: { size?: number }) {
  return (
    <svg
      width={props.size ?? 18}
      height={props.size ?? 18}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.9"
      stroke-linecap="round"
      aria-hidden="true"
    >
      <path d="M12 21V8.5" />
      <circle cx="12" cy="9.5" r="5.5" />
    </svg>
  );
}

// Crisp caption glyphs for the Windows/Linux window controls (10 px grid).
export function IconWinMinimize(props: { size?: number }) {
  return (
    <svg
      width={props.size ?? 10}
      height={props.size ?? 10}
      viewBox="0 0 10 10"
      fill="none"
      stroke="currentColor"
      stroke-width="1"
      aria-hidden="true"
    >
      <path d="M0 5.5h10" />
    </svg>
  );
}

export function IconWinMaximize(props: { size?: number }) {
  return (
    <svg
      width={props.size ?? 10}
      height={props.size ?? 10}
      viewBox="0 0 10 10"
      fill="none"
      stroke="currentColor"
      stroke-width="1"
      aria-hidden="true"
    >
      <rect x="0.5" y="0.5" width="9" height="9" />
    </svg>
  );
}

export function IconWinRestore(props: { size?: number }) {
  return (
    <svg
      width={props.size ?? 10}
      height={props.size ?? 10}
      viewBox="0 0 10 10"
      fill="none"
      stroke="currentColor"
      stroke-width="1"
      aria-hidden="true"
    >
      <path d="M2.5 2.5V0.5h7v7h-2" />
      <rect x="0.5" y="2.5" width="7" height="7" />
    </svg>
  );
}

export function IconWinClose(props: { size?: number }) {
  return (
    <svg
      width={props.size ?? 10}
      height={props.size ?? 10}
      viewBox="0 0 10 10"
      fill="none"
      stroke="currentColor"
      stroke-width="1"
      aria-hidden="true"
    >
      <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
    </svg>
  );
}
