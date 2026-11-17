import createMDX from "@next/mdx";
import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "export",
  distDir: "out",
  images: { unoptimized: true },
  // PostHog's `/e/e/` must reach its rewrite as it is: a redirect first can lose a click's beacon.
  // vercel.json redirects every other trailing slash.
  skipTrailingSlashRedirect: true,
};

// The user manual (content/docs/*.mdx): GitHub-flavored Markdown (tables) and linkable headings. Plugins go by
// name so Turbopack can load them.
const withMDX = createMDX({
  options: { remarkPlugins: ["remark-gfm"], rehypePlugins: ["rehype-slug"] },
});

export default withMDX(nextConfig);
