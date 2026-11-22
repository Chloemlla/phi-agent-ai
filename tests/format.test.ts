import { afterAll, describe, expect, test } from "bun:test";
import { formatCompactTokens, formatElapsed, formatRelative } from "../src/format";
import { setLang } from "../src/i18n";

const NOW = Date.parse("2026-01-01T12:00:00Z");

function secondsAgo(seconds: number): string {
  return new Date(NOW - seconds * 1000).toISOString();
}

afterAll(() => {
  setLang("en");
});

describe("formatCompactTokens", () => {
  test("clamps non-positive and non-finite values to zero", () => {
    expect(formatCompactTokens(0)).toBe("0");
    expect(formatCompactTokens(-12)).toBe("0");
    expect(formatCompactTokens(Number.NaN)).toBe("0");
  });

  test("keeps small counts intact", () => {
    expect(formatCompactTokens(999)).toBe("999");
  });

  test("uses k for thousands and M for millions", () => {
    expect(formatCompactTokens(1234)).toBe("1.2k");
    expect(formatCompactTokens(123456)).toBe("123k");
    expect(formatCompactTokens(1_400_000)).toBe("1.4M");
  });
});

describe("formatElapsed", () => {
  test("formats minutes and hours", () => {
    expect(formatElapsed(0)).toBe("0:00");
    expect(formatElapsed(65)).toBe("1:05");
  });

  test("adds an hour component when needed", () => {
    expect(formatElapsed(3661)).toBe("1:01:01");
  });

  test("clamps negative durations", () => {
    expect(formatElapsed(-3)).toBe("0:00");
  });
});

describe("formatRelative", () => {
  test("localizes fresh timestamps", () => {
    setLang("en");
    expect(formatRelative(secondsAgo(10), NOW)).toBe("just now");
    setLang("zh-CN");
    expect(formatRelative(secondsAgo(10), NOW)).toBe("刚刚");
  });

  test("localizes minutes in both languages", () => {
    setLang("en");
    expect(formatRelative(secondsAgo(5 * 60), NOW)).toBe("5 min ago");
    setLang("zh-CN");
    expect(formatRelative(secondsAgo(5 * 60), NOW)).toBe("5 分钟前");
  });

  test("localizes hours and days", () => {
    setLang("en");
    expect(formatRelative(secondsAgo(3 * 3600), NOW)).toBe("3 h ago");
    expect(formatRelative(secondsAgo(2 * 86400), NOW)).toBe("2 d ago");
    setLang("zh-CN");
    expect(formatRelative(secondsAgo(3 * 3600), NOW)).toBe("3 小时前");
    expect(formatRelative(secondsAgo(2 * 86400), NOW)).toBe("2 天前");
  });

  test("falls back to a locale date beyond a week", () => {
    setLang("en");
    const english = formatRelative(secondsAgo(10 * 86400), NOW);
    expect(english).not.toContain("ago");
    expect(english.length).toBeGreaterThan(0);
    setLang("zh-CN");
    const chinese = formatRelative(secondsAgo(10 * 86400), NOW);
    expect(chinese).not.toContain("前");
    expect(chinese.length).toBeGreaterThan(0);
  });
});
