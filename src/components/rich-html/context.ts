import type { UiValues } from "../uiExpression";

export type RenderContext = {
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
  state: UiValues;
  locals: UiValues;
  assign: (name: string, value: unknown) => void;
  depth: number;
  budget: { nodes: number };
};
