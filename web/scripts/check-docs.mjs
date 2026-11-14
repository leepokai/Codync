// Checks the user manual without a full build: every content/docs/*.mdx page compiles with the site's MDX plugins
// and is listed in app/docs/pages.ts, and every /docs link and #anchor lands on an existing page and heading.
import { compile } from "@mdx-js/mdx";
import { readdir, readFile } from "node:fs/promises";
import rehypeSlug from "rehype-slug";
import remarkGfm from "remark-gfm";

const dir = new URL("../content/docs/", import.meta.url);
const pagesTs = await readFile(new URL("../app/docs/pages.ts", import.meta.url), "utf8");
const listed = new Set([...pagesTs.matchAll(/slug: "([^"]+)"/g)].map((m) => m[1]));
const ids = new Map(); // slug → its heading ids
const links = []; // [slug, href]
const problems = [];

// A rehype plugin that records the page's heading ids and link targets.
const collect = (slug) => () => (tree) => {
  const walk = (node) => {
    if (node.type === "element" && /^h[1-6]$/.test(node.tagName)) ids.get(slug).add(node.properties.id);
    if (node.type === "element" && node.tagName === "a") links.push([slug, String(node.properties.href)]);
    node.children?.forEach(walk);
  };
  walk(tree);
};

for (const file of (await readdir(dir)).filter((f) => f.endsWith(".mdx"))) {
  const slug = file.slice(0, -".mdx".length);
  ids.set(slug, new Set());
  if (!listed.has(slug)) problems.push(`${file}: not listed in app/docs/pages.ts`);
  try {
    await compile(await readFile(new URL(file, dir)), {
      remarkPlugins: [remarkGfm],
      rehypePlugins: [rehypeSlug, collect(slug)],
    });
  } catch (e) {
    problems.push(`${file}: ${e.message}`);
  }
}
for (const slug of listed) if (!ids.has(slug)) problems.push(`app/docs/pages.ts: no content/docs/${slug}.mdx`);

for (const [from, href] of links) {
  const [path, anchor] = href.startsWith("#") ? [`/docs/${from}`, href.slice(1)] : href.split("#");
  if (path !== "/docs" && !path.startsWith("/docs/")) continue;
  const slug = path.replace(/^\/docs\/?/, "").replace(/\/$/, "");
  if (slug === "" && !anchor) continue; // the docs index
  if (!ids.has(slug)) problems.push(`${from}.mdx: ${href} names no page`);
  else if (anchor && !ids.get(slug).has(anchor)) problems.push(`${from}.mdx: ${href} names no heading`);
}

if (problems.length) {
  console.error(problems.join("\n"));
  process.exit(1);
}
console.log(`Docs: ${ids.size} pages and ${links.length} links check out.`);
