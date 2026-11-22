// Bun does not apply Vite's Solid JSX transform to `.tsx` modules under
// `bun test`. This preload plugin compiles those modules with the same
// Babel preset the application build uses, so components render through
// `solid-js/web`'s DOM runtime.
//
// Solid's client build must still be selected with `--conditions=browser`
// (see the `test` script); Bun's default "node" condition resolves
// `solid-js/web` to the server build, which throws on `render`.
// It only affects the test process; application builds still use Vite.
import { plugin } from "bun";
import { transformAsync } from "@babel/core";
import solid from "babel-preset-solid";
import typescript from "@babel/preset-typescript";

plugin({
  name: "phi-solid-test",
  setup(build) {
    build.onLoad(
      { filter: /\.tsx$/, namespace: "file" },
      async ({ path }) => {
        const source = await Bun.file(path).text();
        const result = await transformAsync(source, {
          filename: path,
          babelrc: false,
          configFile: false,
          sourceMaps: "inline",
          presets: [
            [solid, { generate: "dom", hydratable: false }],
            [
              typescript,
              { isTSX: true, allExtensions: true, allowDeclareFields: true },
            ],
          ],
        });
        return { contents: result?.code ?? "", loader: "js" };
      },
    );
  },
});
