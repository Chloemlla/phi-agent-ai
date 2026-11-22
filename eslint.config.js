import js from "@eslint/js";
import jsxA11y from "eslint-plugin-jsx-a11y";
import solid from "eslint-plugin-solid";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["dist/**", "node_modules/**", "references/**", "src-tauri/**", "src/generated/**"],
  },
  {
    files: ["src/**/*.{ts,tsx}", "tests/**/*.ts"],
    extends: [
      js.configs.recommended,
      ...tseslint.configs.recommended,
      solid.configs["flat/typescript"],
      jsxA11y.flatConfigs.recommended,
    ],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: {
        ecmaFeatures: { jsx: true },
        sourceType: "module",
      },
      globals: {
        ...globals.browser,
        ...globals.bunBuiltin,
      },
    },
    rules: {
      // The transcript is a keyboard-scrollable scroll container, and the
      // model/thinking menus manage focus by moving it to their row buttons.
      // Both patterns are intentional, so keep these heuristics as warnings
      // instead of failing CI on them.
      "jsx-a11y/no-noninteractive-tabindex": "warn",
      "jsx-a11y/interactive-supports-focus": "warn",
    },
  },
);
