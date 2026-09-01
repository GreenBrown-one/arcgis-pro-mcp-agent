// @ts-expect-error -- the renderer tsconfig intentionally omits Node types.
import { readFileSync } from "node:fs";
// @ts-expect-error -- the renderer tsconfig intentionally omits Node types.
import { dirname, resolve } from "node:path";
// @ts-expect-error -- the renderer tsconfig intentionally omits Node types.
import { fileURLToPath } from "node:url";

const testDirectory = dirname(fileURLToPath(import.meta.url));
const css = readFileSync(resolve(testDirectory, "../src/app.css"), "utf8");

function rule(source: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = source.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`, "m"));
  expect(match, `missing CSS rule for ${selector}`).not.toBeNull();
  return match![1];
}

function balancedBlock(source: string, header: string): string {
  const headerIndex = source.indexOf(header);
  expect(headerIndex, `missing CSS block for ${header}`).toBeGreaterThanOrEqual(0);

  const openingBrace = source.indexOf("{", headerIndex + header.length);
  expect(openingBrace, `missing opening brace for ${header}`).toBeGreaterThanOrEqual(0);

  let depth = 1;
  for (let index = openingBrace + 1; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(openingBrace + 1, index);
  }

  throw new Error(`missing closing brace for ${header}`);
}

function stripCssComments(source: string): string {
  let uncommented = "";
  let cursor = 0;

  while (cursor < source.length) {
    const commentStart = source.indexOf("/*", cursor);
    if (commentStart === -1) return uncommented + source.slice(cursor);

    uncommented += source.slice(cursor, commentStart);
    const commentEnd = source.indexOf("*/", commentStart + 2);
    if (commentEnd === -1) throw new Error("unterminated CSS comment");
    cursor = commentEnd + 2;
  }

  return uncommented;
}

function narrowContextBlock(source: string): string {
  return balancedBlock(stripCssComments(source), "@media (max-width: 979px)");
}

function expectNarrowContextDrawer(source: string): void {
  const narrow = narrowContextBlock(source);
  expect(rule(narrow, ".context-toggle")).toContain("display: flex");
  expect(rule(narrow, ".context-pane--open")).toContain("visibility: visible");
}

test("root and conversation can shrink without hiding the composer", () => {
  const rootMatch = css.match(/html,\s*body,\s*#root\s*\{([^}]*)\}/m);
  expect(rootMatch).not.toBeNull();
  const root = rootMatch![1];
  expect(root).toContain("height: 100%");
  expect(root).toContain("min-height: 0");
  expect(root).not.toContain("min-height: 720px");
  expect(rule(css, ".app-shell")).toContain("min-height: 0");
  expect(rule(css, ".conversation-body")).toContain("overflow-y: auto");
});

test("narrow effective CSS width retains the context drawer", () => {
  expectNarrowContextDrawer(css);
});

test("drawer assertion does not borrow display flex from context close", () => {
  const withoutToggleDisplay = css.replace(
    /(\.context-toggle\s*\{[^}]*?)display:\s*flex;?/,
    "$1",
  );
  const narrow = narrowContextBlock(withoutToggleDisplay);

  expect(rule(narrow, ".context-toggle")).not.toContain("display: flex");
  expect(rule(narrow, ".context-close")).toContain("display: flex");
  expect(() => expectNarrowContextDrawer(withoutToggleDisplay)).toThrow(/display: flex/);
});

test("drawer assertion ignores a commented toggle declaration", () => {
  const commentedToggleDisplay = css.replace(
    /(\.context-toggle\s*\{[^}]*?)(display:\s*flex;?)/,
    "$1/* $2 */",
  );

  expect(() => expectNarrowContextDrawer(commentedToggleDisplay)).toThrow(/display: flex/);
});

test("drawer assertion ignores a fully commented media block", () => {
  const commentedMedia = `
    /*
    @media (max-width: 979px) {
      .context-toggle { display: flex; }
      .context-pane--open { visibility: visible; }
    }
    */
  `;

  expect(() => expectNarrowContextDrawer(commentedMedia)).toThrow(/missing CSS block/);
});

test("drawer assertion ignores braces inside CSS comments", () => {
  const withCommentedBrace = css.replace(
    "@media (max-width: 979px) {",
    "@media (max-width: 979px) { /* } */",
  );

  expect(() => expectNarrowContextDrawer(withCommentedBrace)).not.toThrow();
});

test("drawer assertion rejects an unterminated media block", () => {
  const unterminatedMedia = `
    @media (max-width: 979px) {
      .context-toggle { display: flex; }
      .context-pane--open { visibility: visible; }
  `;

  expect(() => expectNarrowContextDrawer(unterminatedMedia)).toThrow(/missing closing brace/);
});

test("drawer assertion rejects an unterminated CSS comment", () => {
  const unterminatedComment = css.replace(
    /(\.context-toggle\s*\{[^}]*?)(display:\s*flex;?)/,
    "$1/* $2",
  );

  expect(() => expectNarrowContextDrawer(unterminatedComment)).toThrow(
    /unterminated CSS comment/,
  );
});
