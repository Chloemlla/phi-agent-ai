/* @refresh reload */

// Local-first fonts: bundled at build time, never fetched at runtime.
// Figtree covers Latin, Noto Sans SC covers Chinese, Cascadia Code carries data.
import "@fontsource/figtree/400.css";
import "@fontsource/figtree/500.css";
import "@fontsource/figtree/600.css";
import "@fontsource/figtree/700.css";
import "@fontsource/noto-sans-sc/400.css";
import "@fontsource/noto-sans-sc/500.css";
import "@fontsource/noto-sans-sc/600.css";
import "@fontsource/noto-sans-sc/700.css";
import "@fontsource/cascadia-code/400.css";
import "@fontsource/cascadia-code/600.css";

import { render } from "solid-js/web";
import App from "./App";

render(() => <App />, document.getElementById("root") as HTMLElement);
