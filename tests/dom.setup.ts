// Register a browser-like DOM before any renderer module is imported, so the
// Solid renderers and the inert <template>/DOMParser parsing they rely on run
// under `bun test`.
import { GlobalRegistrator } from "@happy-dom/global-registrator";

if (!GlobalRegistrator.isRegistered) {
  GlobalRegistrator.register({ url: "https://localhost/" });
}

// Solid's client build and JSX transform are not Bun defaults.
import "./solid.bun";
