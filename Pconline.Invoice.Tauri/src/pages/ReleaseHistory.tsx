import { CalendarOutlined, HistoryOutlined } from "@ant-design/icons";
import { Card, Collapse, Space, Tag, Typography } from "antd";
import type { CSSProperties } from "react";
import { MOCK_RELEASE_HISTORY, type ReleaseHistoryItem } from "../data/releaseHistoryMock";

const notesBoxStyle: CSSProperties = {
  margin: 0,
  padding: "12px 14px 12px 16px",
  borderRadius: 8,
  borderLeft: "3px solid rgba(59, 130, 246, 0.85)",
  background: "rgba(15, 23, 42, 0.55)",
  color: "rgba(229, 231, 235, 0.92)",
  lineHeight: 1.65,
};

const listStyle: CSSProperties = {
  margin: 0,
  paddingLeft: 20,
  color: "rgba(229, 231, 235, 0.92)",
  lineHeight: 1.75,
};

function renderNotes(notes: ReleaseHistoryItem["notes"]) {
  if (Array.isArray(notes)) {
    if (notes.length === 0) {
      return (
        <Typography.Paragraph type="secondary" style={{ ...notesBoxStyle, borderLeftColor: "rgba(148, 163, 184, 0.45)" }}>
          No notes.
        </Typography.Paragraph>
      );
    }
    return (
      <div style={notesBoxStyle}>
        <ul style={listStyle}>
          {notes.map((line, i) => (
            <li key={i}>{line}</li>
          ))}
        </ul>
      </div>
    );
  }
  return <Typography.Paragraph style={notesBoxStyle}>{notes}</Typography.Paragraph>;
}

export function ReleaseHistory() {
  const firstVersion = MOCK_RELEASE_HISTORY[0]?.version;

  return (
    <div style={{ flex: 1, minHeight: 0, minWidth: 0, overflow: "auto", width: "100%" }}>
      <Space align="start" size={12} style={{ marginBottom: 12 }}>
        <div
          style={{
            width: 40,
            height: 40,
            borderRadius: 10,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            background: "linear-gradient(145deg, rgba(59,130,246,0.35), rgba(30,58,138,0.45))",
            border: "1px solid rgba(96, 165, 250, 0.35)",
            color: "#93c5fd",
            fontSize: 18,
          }}
        >
          <HistoryOutlined />
        </div>
        <div>
          <Typography.Title level={4} style={{ margin: 0, marginBottom: 4 }}>
            Release history
          </Typography.Title>
        </div>
      </Space>

      <Card
        size="small"
        style={{
          borderColor: "rgba(148, 163, 184, 0.22)",
          background: "rgba(2, 6, 23, 0.35)",
        }}
        styles={{
          body: { padding: "12px 12px 8px" },
        }}
      >
        <Collapse
          bordered={false}
          expandIconPosition="end"
          defaultActiveKey={firstVersion ? [firstVersion] : undefined}
          style={{ background: "transparent" }}
          items={MOCK_RELEASE_HISTORY.map((row, index) => {
            const isLatest = index === 0;
            return {
              key: row.version,
              style: {
                marginBottom: 8,
                borderRadius: 8,
                border: "1px solid rgba(148, 163, 184, 0.18)",
                background: "rgba(15, 23, 42, 0.4)",
                overflow: "hidden",
              },
              label: (
                <Space size={10} wrap style={{ width: "100%" }}>
                  <Tag color="blue" style={{ marginInlineEnd: 0 }}>
                    v{row.version}
                  </Tag>
                  {isLatest ? (
                    <Tag color="processing" style={{ marginInlineEnd: 0 }}>
                      Latest
                    </Tag>
                  ) : null}
                  <Space size={6} style={{ color: "rgba(148, 163, 184, 0.95)" }}>
                    <CalendarOutlined />
                    <Typography.Text type="secondary" style={{ fontSize: 13 }}>
                      {row.pubDate}
                    </Typography.Text>
                  </Space>
                </Space>
              ),
              children: renderNotes(row.notes),
            };
          })}
        />
      </Card>
    </div>
  );
}
