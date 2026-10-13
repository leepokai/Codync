import { APP_STORE, DISCORD, DMG_FILE, GITHUB, PRODUCT_HUNT } from "./links";
import { DESCRIPTION, SITE } from "./site";

// Structured data so search and AI answer engines read Codync as a free, open-source app.
const graph = {
  "@context": "https://schema.org",
  "@graph": [
    {
      "@type": "WebSite",
      "@id": `${SITE}/#website`,
      url: SITE,
      name: "Codync",
      publisher: { "@id": `${SITE}/#org` },
    },
    {
      "@type": "Organization",
      "@id": `${SITE}/#org`,
      name: "Codync",
      url: SITE,
      logo: `${SITE}/icon.png`,
      sameAs: [GITHUB, APP_STORE, PRODUCT_HUNT, DISCORD],
    },
    {
      "@type": "SoftwareApplication",
      "@id": `${SITE}/#app`,
      name: "Codync",
      url: SITE,
      description: DESCRIPTION,
      applicationCategory: "DeveloperApplication",
      operatingSystem: "macOS, Windows, Linux, iOS",
      image: `${SITE}/icon.png`,
      downloadUrl: [DMG_FILE, APP_STORE],
      installUrl: APP_STORE,
      license: "https://www.apache.org/licenses/LICENSE-2.0",
      isAccessibleForFree: true,
      offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
      publisher: { "@id": `${SITE}/#org` },
      sameAs: [GITHUB, APP_STORE],
    },
  ],
};

export default function JsonLd() {
  return (
    <script
      type="application/ld+json"
      dangerouslySetInnerHTML={{ __html: JSON.stringify(graph).replace(/</g, "\\u003c") }}
    />
  );
}
