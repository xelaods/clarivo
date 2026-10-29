// EasyUI の Command Menu（MIT, https://easyui.site/components/command-menu）を、
// 外から項目を渡せる形にして日本語化したもの。Ctrl+K で開く。

import React, { useEffect, useMemo, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Search, CornerDownLeft } from "lucide-react";
import { cn } from "@/lib/utils";
import { motionTransitions } from "@/lib/motion-tokens";

export interface CommandItem {
  id: string;
  title: string;
  /** 見出し（例：「ページ」「Game のプリセット」） */
  category: string;
  icon?: React.ReactNode;
  /** 右端に出す補足（現在の状態やショートカット） */
  hint?: string;
  /** 検索に使う追加の語 */
  keywords?: string;
  onSelect: () => void;
}

export interface CommandMenuProps {
  isOpen: boolean;
  onClose: () => void;
  items: CommandItem[];
  placeholder?: string;
}

export const CommandMenu: React.FC<CommandMenuProps> = ({ isOpen, onClose, items, placeholder = "操作やプリセットを検索" }) => {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);

  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSelectedIndex(0);
    }
  }, [isOpen]);

  const filteredItems = useMemo(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (words.length === 0) return items;
    return items.filter((item) => {
      const hay = `${item.title} ${item.category} ${item.keywords ?? ""}`.toLowerCase();
      return words.every((w) => hay.includes(w));
    });
  }, [items, query]);

  const run = (item: CommandItem) => {
    item.onSelect();
    onClose();
  };

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev + 1) % (filteredItems.length || 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev - 1 + filteredItems.length) % (filteredItems.length || 1));
      } else if (e.key === "Enter" && filteredItems[selectedIndex]) {
        e.preventDefault();
        run(filteredItems[selectedIndex]);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, filteredItems, selectedIndex, onClose]);

  // 見出しごとにまとめる（並び順は items の順）
  let lastCategory = "";

  return (
    <AnimatePresence>
      {isOpen && (
        <div className="fixed inset-0 z-50 flex items-start justify-center px-4 pt-20 sm:px-6" role="dialog" aria-modal="true" aria-label="コマンドメニュー">
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={onClose}
            className="fixed inset-0 bg-black/60 backdrop-blur-sm"
          />
          <motion.div
            initial={{ opacity: 0, scale: 0.97, y: -8 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.97, y: -8 }}
            transition={motionTransitions.springSnappy}
            className="relative z-10 w-full max-w-xl overflow-hidden rounded-xl border border-[#1F1F1F] bg-[#141414] shadow-[0_24px_60px_rgba(0,0,0,0.6)]"
          >
            <div className="flex items-center border-b border-[#1F1F1F] px-4 py-3">
              <Search className="mr-2.5 h-4 w-4 shrink-0 text-[#525252]" />
              <input
                autoFocus
                type="text"
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setSelectedIndex(0);
                }}
                placeholder={placeholder}
                className="w-full bg-transparent text-[15px] text-[#FAFAFA] placeholder-[#6B6B6B] focus:outline-none"
              />
              <span className="rounded border border-[#1F1F1F] bg-[#0E0E0E] px-1.5 py-0.5 font-mono text-[10px] text-[#6B6B6B]">ESC</span>
            </div>

            <div className="max-h-[360px] overflow-y-auto p-1.5" role="listbox">
              {filteredItems.length === 0 ? (
                <div className="py-8 text-center text-[12px] text-[#6B6B6B]">「{query}」に一致する操作はありません</div>
              ) : (
                <div className="space-y-0.5">
                  {filteredItems.map((item, idx) => {
                    const isSelected = idx === selectedIndex;
                    const header = item.category !== lastCategory ? item.category : null;
                    lastCategory = item.category;
                    return (
                      <React.Fragment key={item.id}>
                        {header && <div className="px-3 pb-1 pt-2.5 text-[11px] font-medium text-[#6B6B6B]">{header}</div>}
                        <button
                          onClick={() => run(item)}
                          onMouseEnter={() => setSelectedIndex(idx)}
                          role="option"
                          aria-selected={isSelected}
                          className={cn(
                            "flex w-full items-center justify-between rounded-md px-3 py-2 text-left text-[13px] transition-colors",
                            isSelected ? "bg-[#1F1F1F] text-[#FAFAFA]" : "text-[#A1A1A1] hover:bg-[#0E0E0E]",
                          )}
                        >
                          <div className="flex min-w-0 items-center gap-2.5">
                            {item.icon && <span className="shrink-0 text-[#A1A1A1]">{item.icon}</span>}
                            <span className="truncate">{item.title}</span>
                          </div>
                          <div className="flex shrink-0 items-center gap-2">
                            {item.hint && <span className="text-[11px] text-[#6B6B6B]">{item.hint}</span>}
                            {isSelected && <CornerDownLeft className="h-3 w-3 text-white" />}
                          </div>
                        </button>
                      </React.Fragment>
                    );
                  })}
                </div>
              )}
            </div>

            <div className="flex items-center justify-between border-t border-[#1F1F1F] bg-[#141414] px-4 py-2 text-[11px] text-[#6B6B6B]">
              <div className="flex items-center gap-3">
                <span className="flex items-center gap-1">
                  <span className="rounded border border-[#1F1F1F] bg-[#0E0E0E] px-1 font-mono text-[10px] text-[#A1A1A1]">↑↓</span> 移動
                </span>
                <span className="flex items-center gap-1">
                  <span className="rounded border border-[#1F1F1F] bg-[#0E0E0E] px-1 font-mono text-[10px] text-[#A1A1A1]">↵</span> 実行
                </span>
              </div>
              <span>Ctrl+K で開く</span>
            </div>
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
};
