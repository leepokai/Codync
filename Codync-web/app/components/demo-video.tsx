"use client";

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

const VIDEOS = [
  {
    id: "A5ki29svIc4",
    title: "App Overview",
    description: "Full walkthrough of Codync on Mac & iPhone",
    orientation: "landscape" as const,
  },
  {
    id: "9klpDsbHwJU",
    title: "Real Device Demo",
    description: "iCloud sync between Mac, iPhone & Watch",
    orientation: "portrait" as const,
  },
  {
    id: "N-qhrJugoZo",
    title: "Apple Watch",
    description: "Live session monitoring on your wrist",
    orientation: "portrait" as const,
  },
];

export default function DemoVideo() {
  const [activeVideo, setActiveVideo] = useState<string | null>(null);

  return (
    <>
      <section className="px-6 py-16">
        <div className="max-w-4xl mx-auto">
          <motion.h2
            initial={{ opacity: 0 }}
            whileInView={{ opacity: 1 }}
            viewport={{ once: true }}
            className="text-2xl font-bold text-white text-center mb-8"
          >
            See it in action
          </motion.h2>

          <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
            {VIDEOS.map((video, i) => (
              <motion.button
                key={video.id}
                initial={{ opacity: 0, y: 20 }}
                whileInView={{ opacity: 1, y: 0 }}
                viewport={{ once: true }}
                transition={{ duration: 0.4, delay: i * 0.1 }}
                onClick={() => setActiveVideo(video.id)}
                className="group relative rounded-2xl border border-neutral-800 bg-neutral-950 overflow-hidden text-left transition-colors hover:border-neutral-600"
              >
                <div className="relative aspect-video overflow-hidden">
                  <img
                    src={`https://img.youtube.com/vi/${video.id}/hqdefault.jpg`}
                    alt={video.title}
                    className="w-full h-full object-cover transition-transform duration-300 group-hover:scale-105"
                  />
                  <div className="absolute inset-0 flex items-center justify-center bg-black/30 group-hover:bg-black/20 transition-colors">
                    <div className="w-12 h-12 rounded-full bg-white/90 flex items-center justify-center shadow-lg group-hover:scale-110 transition-transform">
                      <svg viewBox="0 0 24 24" fill="#000" className="w-5 h-5 ml-0.5">
                        <path d="M8 5v14l11-7z" />
                      </svg>
                    </div>
                  </div>
                </div>
                <div className="p-4">
                  <p className="text-sm font-semibold text-white">{video.title}</p>
                  <p className="text-xs text-neutral-500 mt-1">{video.description}</p>
                </div>
              </motion.button>
            ))}
          </div>
        </div>
      </section>

      {/* Modal */}
      <AnimatePresence>
        {activeVideo && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4"
            onClick={() => setActiveVideo(null)}
          >
            <motion.div
              initial={{ scale: 0.9, opacity: 0 }}
              animate={{ scale: 1, opacity: 1 }}
              exit={{ scale: 0.9, opacity: 0 }}
              transition={{ duration: 0.2 }}
              onClick={(e) => e.stopPropagation()}
              className={
                VIDEOS.find((v) => v.id === activeVideo)?.orientation === "portrait"
                  ? "relative w-full max-w-[360px] aspect-[9/16] rounded-2xl overflow-hidden"
                  : "relative w-full max-w-3xl aspect-video rounded-2xl overflow-hidden"
              }
            >
              <iframe
                src={`https://www.youtube.com/embed/${activeVideo}?autoplay=1`}
                title="Codync Demo"
                allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                allowFullScreen
                className="absolute inset-0 w-full h-full"
              />
              <button
                onClick={() => setActiveVideo(null)}
                className="absolute top-3 right-3 w-8 h-8 rounded-full bg-black/60 flex items-center justify-center text-white hover:bg-black/80 transition-colors"
              >
                ✕
              </button>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}
