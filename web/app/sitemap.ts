import type { MetadataRoute } from "next";
import { rivals } from "./compare/rivals";
import { agentPages } from "./agents/agents";
import { docPages } from "./docs/pages";
import { SITE } from "./site";

export const dynamic = "force-static";

export default function sitemap(): MetadataRoute.Sitemap {
  const pages: [path: string, priority: number][] = [
    ["", 1],
    ["/compare", 0.8],
    ...rivals.map((r): [string, number] => [`/compare/${r.slug}`, 0.7]),
    ["/free-ai-agent-app", 0.9],
    ...agentPages.map((a): [string, number] => [`/${a.slug}`, 0.8]),
    ["/codync-mem", 0.7],
    ["/wechat", 0.5],
    ["/docs", 0.8],
    ...docPages.map((p): [string, number] => [`/docs/${p.slug}`, 0.6]),
    ["/privacy", 0.2],
    ["/terms", 0.2],
  ];
  return pages.map(([path, priority]) => ({ url: `${SITE}${path}`, lastModified: new Date(), priority }));
}
