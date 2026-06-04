import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, confirm } from "@tauri-apps/plugin-dialog";
import { Table, Card, Row, Col, Space, Typography, Select, Button, Input, List, message, Progress } from "antd";
import {
  FileAddOutlined,
  CheckOutlined,
  UploadOutlined,
  DeleteOutlined,
  ClearOutlined,
} from "@ant-design/icons";
import type { ColumnsType } from "antd/es/table";
import { appendTimedLog } from "../utils/logging";

type Carrier = "Amazon" | "Temu" | "UPS" | "Fedex";

interface FileInfoDto {
  fileFullName: string;
  fileName: string;
  status: string;
  rowCount: number;
  carrier: string;
  accountNumber?: string | null;
  invoiceNumber?: string | null;
}

function fileRowKey(f: FileInfoDto): string {
  return `${f.fileFullName}\0${f.fileName}`;
}

function statusClass(s: string): string {
  if (s === "Pass" || s.startsWith("Pass")) return "status-pass";
  if (s === "Exists" || s.includes("Exists")) return "status-exists";
  if (s === "Successful") return "status-successful";
  if (s === "Failed" || s.startsWith("Error") || s.startsWith("Check error")) return "status-failed status-error";
  return "";
}

export function InvoiceUpload() {
  const [carrier, setCarrier] = useState<Carrier | "">("");
  const [files, setFiles] = useState<FileInfoDto[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [activeFileKey, setActiveFileKey] = useState<string | null>(null);
  const [activePhase, setActivePhase] = useState<"check" | "upload" | null>(null);

  const { Title, Text } = Typography;
  const { TextArea } = Input;

  const appendLog = (msg: string, newLine = false): void => {
    appendTimedLog(setLogs, msg, newLine);
  };

  /** 按索引顺序处理文件：高亮行、刷新表格；仅 invoke 抛错时写日志并标 Error */
  const runSequential = async (
    phase: "check" | "upload",
    draft: FileInfoDto[],
    indices: number[],
    processOne: (idx: number, draft: FileInfoDto[]) => Promise<void>,
  ): Promise<void> => {
    for (const idx of indices) {
      const file = draft[idx];
      setActiveFileKey(fileRowKey(file));
      setActivePhase(phase);
      try {
        await processOne(idx, draft);
      } catch (e) {
        draft[idx] = { ...draft[idx], status: `Error: ${String(e)}` };
        appendLog(`${file.fileName}: ${String(e)}`, true);
      } finally {
        setFiles([...draft]);
        setActiveFileKey(null);
        setActivePhase(null);
      }
    }
    setFiles(draft);
  };

  const handleSelectFiles = async () => {
    if (!carrier) {
      message.warning("Please select a carrier.");
      return;
    }
    const selected = await open({
      multiple: true,
      filters: [{ name: "Excel", extensions: ["xlsx", "xls"] }],
    });
    if (!selected) return;
    const filePaths = Array.isArray(selected) ? selected : [selected];
    setLoading(true);
    appendLog("Loading…");
    try {
      const result = await invoke<FileInfoDto[]>("invoice_process_files", {
        carrier,
        filePaths,
      });
      setFiles((prev) => [...prev, ...result]);
      appendLog(`Loaded ${filePaths.length} file(s).`, true);
    } catch (e) {
      appendLog(String(e), true);
    } finally {
      setLoading(false);
    }
  };

  const handleCheck = async () => {
    if (files.length === 0) {
      message.warning("No files to check.");
      return;
    }
    setLoading(true);
    appendLog("Checking…");
    try {
      const draft = [...files];
      const indices = draft.map((_, i) => i);
      await runSequential("check", draft, indices, async (idx, draft) => {
        const result = await invoke<FileInfoDto[]>("invoice_check_files", {
          carrier,
          files: [draft[idx]],
        });
        draft[idx] = result[0] ?? draft[idx];
      });
      appendLog("Check complete.", true);
    } finally {
      setLoading(false);
      setActiveFileKey(null);
      setActivePhase(null);
    }
  };

  const handleUpload = async () => {
    if (files.length === 0) {
      message.warning("No files to upload.");
      return;
    }
    const confirmed = await confirm("Are you sure you want to upload all files? This cannot be undone.", {
      title: "Upload",
      kind: "warning",
    });
    if (!confirmed) return;
    setLoading(true);
    appendLog("Uploading…");
    try {
      const draft = [...files];
      const indices = draft.map((f, i) => (f.status === "Pass" ? i : -1)).filter((i) => i >= 0);
      await runSequential("upload", draft, indices, async (idx, draft) => {
        const row = draft[idx];
        const checkResult = await invoke<FileInfoDto[]>("invoice_check_files", {
          carrier,
          files: [row],
        });
        const checked = checkResult[0];
        if (!checked || checked.status !== "Pass") {
          draft[idx] = checked ?? { ...row, status: "Error: pre-upload check returned no result" };
          return;
        }
        const uploadResult = await invoke<FileInfoDto[]>("invoice_upload_files", {
          carrier,
          files: [checked],
        });
        draft[idx] = uploadResult[0] ?? draft[idx];
      });
      appendLog("Upload complete.", true);
    } finally {
      setLoading(false);
      setActiveFileKey(null);
      setActivePhase(null);
    }
  };

  const handleRemove = async () => {
    if (!selectedKey) {
      message.warning("Please select a row first.");
      return;
    }
    const confirmed = await confirm("Are you sure you want to delete the current row?", {
      title: "Remove",
      kind: "warning",
    });
    if (!confirmed) return;
    setFiles((prev) => prev.filter((f) => fileRowKey(f) !== selectedKey));
    setSelectedKey(null);
    setActiveFileKey(null);
    setActivePhase(null);
  };

  const handleClear = async () => {
    const confirmed = await confirm("Are you sure you want to clear?", {
      title: "Clear",
      kind: "warning",
    });
    if (!confirmed) return;
    setFiles([]);
    setLogs([]);
    setCarrier("");
    setSelectedKey(null);
    setActiveFileKey(null);
    setActivePhase(null);
  };

  const columns: ColumnsType<FileInfoDto> = [
    {
      title: "Progress",
      key: "progress",
      width: 60,
      render: (_value, record) => {
        const key = fileRowKey(record);
        const isActive = loading && key === activeFileKey;

        const phaseDone =
          activePhase === "upload"
            ? record.status === "Successful" ||
              record.status.startsWith("Failed") ||
              record.status.startsWith("Error") ||
              record.status.startsWith("Exists")
            : activePhase === "check"
              ? record.status !== "Init"
              : record.status !== "Init";

        const percent = phaseDone ? 100 : isActive ? 50 : 0;
        return (
          <Progress type="circle" percent={percent} width={28} strokeWidth={8} status={phaseDone ? "success" : isActive ? "active" : "normal"} format={() => ""} />
        );
      },
    },
    { title: "Carrier", dataIndex: "carrier" },
    { title: "AccountNumber", dataIndex: "accountNumber" },
    { title: "InvoiceNumber", dataIndex: "invoiceNumber" },
    { title: "FileName", dataIndex: "fileName" },
    { title: "RowCount", dataIndex: "rowCount" },
    {
      title: "Status",
      dataIndex: "status",
      render: (text: string) => <span className={statusClass(text)}>{text}</span>,
    },
  ];

  const stepItems = [
    "1. Select Origin.",
    "2. Select the Excel file.",
    "3. Click on \"Check\" button to validate invoice data and formatting.",
    "4. Upload data to the database.",
  ];

  const carrierOptions = [
    { value: "", label: "Select Origin" },
    { value: "Amazon", label: "Amazon Shipping" },
    { value: "Fedex", label: "Fedex" },
    { value: "Temu", label: "Temu" },
    { value: "UPS", label: "UPS" },
  ];

  const canOperate = files.length > 0;
  const allPass = files.length > 0 && files.every((f) => f.status === "Pass");

  return (
    <div className="invoice-upload-page">
      <Space direction="vertical" size={4} style={{ width: "100%" }}>
        <Title level={3} style={{ marginBottom: 0 }}>
          PCO Invoice Upload Tools
        </Title>
      </Space>

      <Row gutter={16} style={{ flex: "0 0 auto" }}>
        <Col flex="1 1 0" style={{ minWidth: 0 }}>
          <Card size="small" style={{ borderRadius: 12 }} bodyStyle={{ padding: 16 }}>
            <Space direction="vertical" size="small" style={{ width: "100%" }}>
              <Space direction="vertical" size={4}>
                <Text strong>Steps</Text>
                <List
                  size="small"
                  dataSource={stepItems}
                  renderItem={(item) => (
                    <List.Item style={{ paddingInline: 0, borderBlock: "none" }}>{item}</List.Item>
                  )}
                />
              </Space>

              <Space align="center" wrap>
                <Select
                  style={{ minWidth: 200 }}
                  value={carrier}
                  onChange={(value) => setCarrier(value as Carrier | "")}
                  options={carrierOptions}
                />
                <Button
                  type="primary"
                  icon={<FileAddOutlined />}
                  onClick={handleSelectFiles}
                  disabled={!carrier || loading}
                >
                  Selected Files
                </Button>
                <Button
                  icon={<CheckOutlined />}
                  onClick={handleCheck}
                  disabled={!canOperate || loading}
                >
                  Check
                </Button>
                <Button
                  icon={<DeleteOutlined />}
                  onClick={handleRemove}
                  disabled={!canOperate || loading || !selectedKey}
                >
                  Remove
                </Button>
                <Button
                  type="primary"
                  icon={<UploadOutlined />}
                  onClick={handleUpload}
                  disabled={!canOperate || loading || !allPass}
                >
                  Upload
                </Button>
                <Button
                  danger
                  icon={<ClearOutlined />}
                  onClick={handleClear}
                  disabled={loading && !canOperate}
                >
                  Clear
                </Button>
              </Space>
            </Space>
          </Card>
        </Col>
        <Col flex="500px">
          <Card size="small" bodyStyle={{ padding: 8 }}>
            <Space direction="vertical" size="small" style={{ width: "100%" }}>
              <TextArea
                readOnly
                value={logs.join("\n")}
                autoSize={false}
                className="logs-textarea"
                rows={10}
                style={{ resize: "none" }}
              />
            </Space>
          </Card>
        </Col>
      </Row>

      <Card style={{ flex: "1 1 auto", minHeight: 0 }}>
        <Table<FileInfoDto>
          columns={columns}
          dataSource={files}
          rowKey={fileRowKey}
          pagination={false}
          size="small"
          rowClassName={(record) => (selectedKey === fileRowKey(record) ? "row-selected" : "")}
          onRow={(record) => ({
            onClick: () => setSelectedKey(fileRowKey(record)),
          })}
        />
      </Card>
    </div>
  );
}
