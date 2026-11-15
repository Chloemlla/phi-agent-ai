// UI icons come from Lucide (stroke icons on a 24 viewBox). The brand mark is
// the only custom glyph: φ, per the visual spec.

export {
  ArrowLeft as IconArrowLeft,
  ArrowUp as IconArrowUp,
  Check as IconCheck,
  ChevronRight as IconChevron,
  CircleAlert as IconCircleAlert,
  Eye as IconEye,
  EyeOff as IconEyeOff,
  KeyRound as IconKey,
  PanelLeft as IconPanel,
  Plus as IconPlus,
  RefreshCw as IconRefresh,
  Server as IconServer,
  Settings as IconSettings,
  Trash2 as IconTrash,
  TriangleAlert as IconAlert,
  Wrench as IconWrench,
  X as IconClose,
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
