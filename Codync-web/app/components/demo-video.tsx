"use client";

import { motion } from "framer-motion";

// Replace with actual YouTube video ID once uploaded
const YOUTUBE_VIDEO_ID = "A5ki29svIc4";

export default function DemoVideo() {
  if (!YOUTUBE_VIDEO_ID) return null;

  return (
    <section className="px-6 py-16">
      <div className="max-w-3xl mx-auto">
        <motion.h2
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          className="text-2xl font-bold text-white text-center mb-8"
        >
          See it in action
        </motion.h2>

        <motion.div
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.6 }}
          className="relative w-full aspect-video rounded-2xl overflow-hidden border border-neutral-800 bg-neutral-950"
        >
          <iframe
            src={`https://www.youtube.com/embed/${YOUTUBE_VIDEO_ID}`}
            title="Codync Demo"
            allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
            allowFullScreen
            className="absolute inset-0 w-full h-full"
          />
        </motion.div>
      </div>
    </section>
  );
}
