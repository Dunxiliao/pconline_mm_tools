import type { Dispatch, SetStateAction } from "react";

export function formatLogTime(date: Date = new Date()): string {
  return date.toISOString().replace("T", " ").slice(0, 19);
}

export function appendTimedLog(
  setLogs: Dispatch<SetStateAction<string[]>>,
  msg: string,
  newLine = false,
): void {
  const time = formatLogTime();
  setLogs((prev) => [...prev, `${time} ${msg}`, ...(newLine ? [""] : [])]);
}

