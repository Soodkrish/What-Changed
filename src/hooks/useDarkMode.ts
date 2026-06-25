import { useState, useEffect } from "react";
import { getSetting, setSetting } from "../lib/tauri";

export function useDarkMode() {
  const [dark, setDark] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getSetting("dark_mode").then((v) => {
      const isDark = v === "true";
      setDark(isDark);
      applyDarkMode(isDark);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, []);

  const toggle = async () => {
    const next = !dark;
    setDark(next);
    applyDarkMode(next);
    await setSetting("dark_mode", String(next));
  };

  return { dark, toggle, loading };
}

function applyDarkMode(isDark: boolean) {
  if (isDark) {
    document.documentElement.classList.add("dark");
  } else {
    document.documentElement.classList.remove("dark");
  }
}
