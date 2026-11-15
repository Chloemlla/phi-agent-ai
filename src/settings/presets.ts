// Quick-start presets for the provider editor. They only fill the form; the
// node still validates and persists everything on save.

export type ProviderPreset = {
  key: string;
  name: string;
  description: string;
  protocol: "anthropic_messages" | "openai_chat" | "openai_responses";
  auth: "x_api_key" | "bearer" | "none";
  baseUrl: string;
  models: string[];
  websiteUrl?: string;
};

export const PRESETS: ProviderPreset[] = [
  {
    key: "anthropic",
    name: "Anthropic",
    description: "Anthropic Messages API.",
    protocol: "anthropic_messages",
    auth: "x_api_key",
    baseUrl: "https://api.anthropic.com",
    models: ["claude-sonnet-5", "claude-opus-5", "claude-haiku-4-5-20251001"],
    websiteUrl: "https://www.anthropic.com",
  },
  {
    key: "openai-chat",
    name: "OpenAI",
    description: "OpenAI Chat Completions API.",
    protocol: "openai_chat",
    auth: "bearer",
    baseUrl: "https://api.openai.com/v1",
    models: ["gpt-5", "gpt-5-mini", "gpt-5-nano"],
    websiteUrl: "https://platform.openai.com",
  },
  {
    key: "openai-responses",
    name: "OpenAI Responses",
    description: "OpenAI Responses API.",
    protocol: "openai_responses",
    auth: "bearer",
    baseUrl: "https://api.openai.com/v1",
    models: ["gpt-5", "gpt-5-mini"],
    websiteUrl: "https://platform.openai.com",
  },
  {
    key: "compatible",
    name: "OpenAI-compatible / local",
    description:
      "Any OpenAI-compatible endpoint, including local runtimes such as Ollama or LM Studio.",
    protocol: "openai_chat",
    auth: "none",
    baseUrl: "http://localhost:11434/v1",
    models: [],
  },
  {
    key: "anthropic-gateway",
    name: "Anthropic-compatible gateway",
    description: "An Anthropic Messages-compatible gateway or proxy.",
    protocol: "anthropic_messages",
    auth: "x_api_key",
    baseUrl: "",
    models: [],
  },
];
