import { describe, expect, test } from "bun:test";

import { isEditableElement } from "./keyboard";

// Lightweight stand-in for the DOM `Element` API. We only need `tagName`
// and `isContentEditable` for `isEditableElement`.
function mk(tagName: string, isContentEditable = false): Element {
  return {
    tagName: tagName.toUpperCase(),
    isContentEditable,
  } as unknown as Element;
}

describe("isEditableElement", () => {
  test("flags input / textarea / select as editable", () => {
    expect(isEditableElement(mk("input"))).toBe(true);
    expect(isEditableElement(mk("textarea"))).toBe(true);
    expect(isEditableElement(mk("select"))).toBe(true);
  });

  test("flags contenteditable elements as editable", () => {
    expect(isEditableElement(mk("div", true))).toBe(true);
    expect(isEditableElement(mk("span", true))).toBe(true);
  });

  test("non-editable elements are left alone", () => {
    expect(isEditableElement(mk("div"))).toBe(false);
    expect(isEditableElement(mk("button"))).toBe(false);
    expect(isEditableElement(mk("a"))).toBe(false);
  });

  test("null never claims editable", () => {
    expect(isEditableElement(null)).toBe(false);
  });

  test("case-insensitive on tag name", () => {
    expect(isEditableElement(mk("Input"))).toBe(true);
    expect(isEditableElement(mk("TEXTAREA"))).toBe(true);
  });
});
