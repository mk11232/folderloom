import { readFile } from "node:fs/promises";
import ts from "typescript";

export async function componentUrl(name) {
  const source = await readFile(new URL(`../src/${name}.tsx`, import.meta.url), "utf8");
  let compiled = ts.transpileModule(source, { compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
  for (const match of [...compiled.matchAll(/from "([^"]+)"/g)]) {
    const module = match[1];
    let url;
    if (module === "./api") url = "data:text/javascript,export const api = {}";
    else if (module === "@tauri-apps/plugin-dialog") url = "data:text/javascript,export const open = () => Promise.resolve(null)";
    else if (module === "./PlanPreview") url = await componentUrl("PlanPreview");
    else if (module === "./merge-layout") url = new URL("../src/merge-layout.ts", import.meta.url).href;
    else url = import.meta.resolve(module);
    compiled = compiled.replaceAll(match[0], `from ${JSON.stringify(url)}`);
  }
  return `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`;
}
