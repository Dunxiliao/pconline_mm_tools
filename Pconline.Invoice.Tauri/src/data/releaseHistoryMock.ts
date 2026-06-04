/**
 * 历史版本列表（占位数据）。
 * 后续可改为：invoke 拉取 API、或 fetch 远程 JSON，再替换本文件的导出。
 */
export interface ReleaseHistoryItem {
  version: string;
  pubDate: string;
  /** 单行摘要，或要点列表（API 返回数组时与字符串等价处理） */
  notes: string | string[];
}

export const MOCK_RELEASE_HISTORY: ReleaseHistoryItem[] = [
  {
    version: "2.3.6",
    pubDate: "2026-06-04",
    notes: [
      "记住登录用户名和密码"
    ],
  },
  {
    version: "2.3.5",
    pubDate: "2026-05-21",
    notes: [
      "增加历史版本记录"
    ],
  },
  {
    version: "2.3.4",
    pubDate: "2026-04-30",
    notes: [
      "修复amazon invoice上传问题"
    ],
  }
];
