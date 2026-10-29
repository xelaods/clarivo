// ゲーム中に画面上部へ一瞬出す表示（ミュートやプリセットの切り替えを知らせる）。

import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { listen } from "./lib/api";
import { motionTransitions } from "./lib/motion-tokens";

export function Overlay() {
  const [msg, setMsg] = useState<{ text: string; key: number } | null>(null);

  useEffect(() => {
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
    let n = 0;
    const off = listen<string>("overlay-message", (text) => {
      n += 1;
      setMsg({ text, key: n });
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!msg) return;
    const id = window.setTimeout(() => setMsg(null), 1500);
    return () => window.clearTimeout(id);
  }, [msg]);

  return (
    <div className="flex h-screen items-start justify-center pt-2">
      <AnimatePresence>
        {msg && (
          <motion.div
            key={msg.key}
            initial={{ opacity: 0, y: -8, scale: 0.97 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -6 }}
            transition={motionTransitions.springSnappy}
            className="flex h-11 items-center gap-2.5 rounded-full border border-border bg-[rgba(17,17,19,0.92)] px-5 text-[14px] font-medium text-text-primary shadow-[0_10px_30px_rgba(0,0,0,0.5)]"
          >
            <span className="h-1.5 w-1.5 rounded-full bg-accent-blue" />
            {msg.text}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
