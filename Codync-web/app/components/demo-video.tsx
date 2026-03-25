"use client";

import { motion } from "framer-motion";

const DEMO_VIDEO_ID = "A5ki29svIc4";
const WATCH_DEMO_VIDEO_ID = "N-qhrJugoZo";

export default function DemoVideo() {
  return (
    <section className="px-6 py-16">
      <div className="max-w-5xl mx-auto">
        <motion.h2
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          className="text-2xl font-bold text-white text-center mb-8"
        >
          See it in action
        </motion.h2>

        <div className="flex flex-col md:flex-row items-center md:items-stretch justify-center gap-8">
          {/* Main demo — landscape */}
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.6 }}
            className="relative w-full md:flex-1 aspect-video rounded-2xl overflow-hidden border border-neutral-800 bg-neutral-950"
          >
            <iframe
              src={`https://www.youtube.com/embed/${DEMO_VIDEO_ID}`}
              title="Codync Demo"
              allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
              allowFullScreen
              className="absolute inset-0 w-full h-full"
            />
          </motion.div>

          {/* Watch demo — vertical Shorts */}
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.6, delay: 0.15 }}
            className="flex flex-col items-center"
          >
            <p className="text-xs text-neutral-500 font-medium mb-2">Apple Watch</p>
            <div className="relative w-[200px] md:w-[220px] aspect-[9/16] rounded-2xl overflow-hidden border border-neutral-800 bg-neutral-950">
              <iframe
                src={`https://www.youtube.com/embed/${WATCH_DEMO_VIDEO_ID}`}
                title="Codync Apple Watch Demo"
                allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                allowFullScreen
                className="absolute inset-0 w-full h-full"
              />
            </div>
          </motion.div>
        </div>
      </div>
    </section>
  );
}
