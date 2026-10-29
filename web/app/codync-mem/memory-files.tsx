// The two layers a bot sees, as the files on disk: each MEMORY.md in full, then its notes' index.
const layers = [
  {
    name: "Yours",
    path: "shared-memory/",
    cap: 400,
    facts: ["Goes by Sam; prefers Traditional Chinese.", "Likes the result first, then the details.", "Wants to be asked before anything goes to main."],
    notes: [{ file: "travel.md", description: "Trips Sam is planning, and how they like to book" }],
  },
  {
    name: "Scout",
    path: "bots/scout/bot-memory/",
    cap: 1200,
    facts: ["Sam reviews pull requests in the morning, Taipei time.", "Sam wants a plan before any large refactor."],
    notes: [{ file: "pace-app.md", description: "The pace project: how Sam wants weeks counted" }],
  },
];

// The crate's tokens::estimate for this English text: one token per four characters.
const estimate = (facts: string[]) => Math.ceil(facts.map((f) => `- ${f}\n`).join("").length / 4);

export default function MemoryFiles() {
  return (
    <div
      role="img"
      aria-label="Two memory folders: Yours, shared by every bot, and Scout's own. Each holds a MEMORY.md of short facts and notes on one topic each."
      className="flex flex-col gap-3 rounded-3xl bg-neutral-900 p-4 md:p-5"
    >
      {layers.map(({ name, path, cap, facts, notes }) => (
        <div key={name} className="rounded-2xl bg-neutral-800 p-4">
          <p className="flex items-baseline gap-2 text-sm">
            <span className="font-medium text-neutral-50">{name}</span>
            <span className="truncate font-mono text-xs text-neutral-500">{path}</span>
          </p>
          <div className="mt-3 rounded-xl bg-black p-3 font-mono text-xs leading-relaxed">
            <p className="flex justify-between text-neutral-500">
              <span>MEMORY.md</span>
              <span>
                {estimate(facts)} / {cap.toLocaleString("en-US")} tokens
              </span>
            </p>
            <ul className="mt-2 text-neutral-200">
              {facts.map((f) => (
                <li key={f}>- {f}</li>
              ))}
            </ul>
          </div>
          <ul className="mt-2 flex flex-col gap-1 px-1 pt-1 text-xs">
            {notes.map(({ file, description }) => (
              <li key={file} className="flex gap-2">
                <span className="shrink-0 font-mono text-neutral-200">{file}</span>
                <span className="truncate text-neutral-500">{description}</span>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </div>
  );
}
