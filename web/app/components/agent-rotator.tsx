"use client";

import { useEffect, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import AgentMark, { agentName, preloadAgentMarks, type Agent } from "./agent-mark";
import { ease } from "./motion";

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
    // The leaving and the arriving agent share one grid cell, so they cross in place.
    <span className="grid">
      <AnimatePresence initial={false}>
        <motion.span
          key={agent}
          initial={{ opacity: 0, y: "0.4em", filter: "blur(10px)" }}
          animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
          exit={{ opacity: 0, y: "-0.4em", filter: "blur(10px)" }}
          transition={{ duration: 0.6, ease }}
          className="col-start-1 row-start-1 inline-flex items-center gap-x-[0.22em] whitespace-nowrap"
        >
          <AgentMark agent={agent} className="size-[0.82em]" />
          {agentName[agent]}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}
