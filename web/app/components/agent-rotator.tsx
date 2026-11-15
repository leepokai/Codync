"use client";

import { useEffect, useState } from "react";
import { useReducedMotion } from "framer-motion";
import AgentMark, { agentName, preloadAgentMarks, type Agent } from "./agent-mark";
import BlurSwap from "./blur-swap";

// One agent at a time, each rising out of a blur; it holds on the first under reduced motion.
export default function AgentRotator({ agents }: { agents: Agent[] }) {
  const [shown, setShown] = useState(0);
  const reduce = useReducedMotion();
  preloadAgentMarks(agents);

  useEffect(() => {
    if (reduce) return;
    const timer = setInterval(() => setShown((i) => (i + 1) % agents.length), 2400);
    return () => clearInterval(timer);
  }, [reduce, agents.length]);

  const agent = agents[shown];
  return (
    <BlurSwap id={agent} className="inline-flex items-center gap-x-[0.22em] justify-self-center whitespace-nowrap">
      <AgentMark agent={agent} className="size-[0.82em]" />
      {agentName[agent]}
    </BlurSwap>
  );
}
