// Host-owned icon vocabulary for inline interface blocks. Names are stable
// model-facing tokens; the components themselves come from lucide-solid.
// Keeping the map here limits the bundle to the icons the prompt advertises.

import {
  AlertTriangle,
  ArrowRight,
  Bike,
  BookOpen,
  Calendar,
  Camera,
  Check,
  ChevronRight,
  CircleCheck,
  Clock,
  ExternalLink,
  FileText,
  Heart,
  Info,
  Lightbulb,
  MapPin,
  Minus,
  Play,
  Plus,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  Star,
  Wrench,
  X,
  type LucideIcon,
} from "lucide-solid";

export const HOST_ICONS: Record<string, LucideIcon> = {
  "alert-triangle": AlertTriangle,
  "arrow-right": ArrowRight,
  bike: Bike,
  "book-open": BookOpen,
  calendar: Calendar,
  camera: Camera,
  check: Check,
  "chevron-right": ChevronRight,
  "circle-check": CircleCheck,
  clock: Clock,
  "external-link": ExternalLink,
  "file-text": FileText,
  heart: Heart,
  info: Info,
  lightbulb: Lightbulb,
  "map-pin": MapPin,
  minus: Minus,
  play: Play,
  plus: Plus,
  search: Search,
  settings: Settings,
  "shield-check": ShieldCheck,
  sparkles: Sparkles,
  star: Star,
  wrench: Wrench,
  x: X,
};

/** Icons the prompt is allowed to advertise. */
export const HOST_ICON_NAMES = new Set(Object.keys(HOST_ICONS));

const ICON_SIZES: Record<string, number> = {
  xs: 12,
  sm: 14,
  md: 16,
  lg: 20,
  xl: 24,
  "2xl": 28,
};

export function hostIconSize(value: string | null): number {
  return ICON_SIZES[value ?? ""] ?? 16;
}
