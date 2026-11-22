import { afterAll, describe, expect, test } from "bun:test";
import { setLang, tPlural } from "../src/i18n";
import { formatList, sentenceCase } from "../src/format";

afterAll(() => setLang("en"));

describe("tPlural", () => {
  test("selects the English one/other form", () => {
    setLang("en");
    expect(tPlural("Read {n} file", "Read {n} files", 1)).toBe("Read 1 file");
    expect(tPlural("Read {n} file", "Read {n} files", 3)).toBe("Read 3 files");
  });

  test("uses the translated catalog for Chinese", () => {
    setLang("zh-CN");
    expect(tPlural("Read {n} file", "Read {n} files", 1)).toBe("读取了 1 个文件");
    expect(tPlural("Read {n} file", "Read {n} files", 3)).toBe("读取了 3 个文件");
  });
});

describe("formatList", () => {
  test("joins parts with the locale conjunction", () => {
    setLang("en");
    expect(formatList(["a", "b", "c"])).toBe("a, b, and c");
    setLang("zh-CN");
    expect(formatList(["a", "b", "c"])).toBe("a、b和c");
  });

  test("capitalizes only English sentences", () => {
    setLang("en");
    expect(sentenceCase("read 2 files")).toBe("Read 2 files");
    setLang("zh-CN");
    expect(sentenceCase("读取 2 个文件")).toBe("读取 2 个文件");
  });
});
