"use client";

import { AnimatePresence, motion } from "framer-motion";
import { ease } from "./motion";

// Swaps its content whenever `id` changes, the new one rising out of a blur.
// The leaving and the arriving content share one grid cell, so they cross in place.
export default function BlurSwap({ id, className, children }: { id: string; className?: string; children: React.ReactNode }) {
  return (
    <span className="grid">
      <AnimatePresence initial={false}>
        <motion.span
          key={id}
          initial={{ opacity: 0, y: "0.4em", filter: "blur(10px)" }}
          animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
          exit={{ opacity: 0, y: "-0.4em", filter: "blur(10px)" }}
          transition={{ duration: 0.6, ease }}
          className={`col-start-1 row-start-1 ${className ?? ""}`}
        >
          {children}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}
