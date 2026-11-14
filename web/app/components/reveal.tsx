"use client";

import { motion, MotionConfig } from "framer-motion";
import { ease } from "./motion";

// Fades a section in once as it scrolls into view. The markup is the same on server and client
// (branching on reduced motion left the server's opacity: 0 in place after hydration);
// MotionConfig drops the movement for reduced motion.
export default function Reveal({
  children,
  delay = 0,
  className,
}: {
  children: React.ReactNode;
  delay?: number;
  className?: string;
}) {
  return (
    <MotionConfig reducedMotion="user">
      <motion.div
        className={className}
        initial={{ opacity: 0, y: 24 }}
        whileInView={{ opacity: 1, y: 0 }}
        viewport={{ once: true, amount: 0.25 }}
        transition={{ duration: 0.6, delay, ease }}
      >
        {children}
      </motion.div>
    </MotionConfig>
  );
}
