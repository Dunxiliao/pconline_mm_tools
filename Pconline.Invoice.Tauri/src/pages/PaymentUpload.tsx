import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, confirm } from "@tauri-apps/plugin-dialog";
import { Table, Card, Row, Col, Space, Typography, Button, Input, List, message, Tabs, Progress } from "antd";
import {
  FileAddOutlined,
  CheckOutlined,
  UploadOutlined,
  DeleteOutlined,
  ClearOutlined,
} from "@ant-design/icons";
import type { ColumnsType } from "antd/es/table";
import { appendTimedLog } from "../utils/logging";

export interface OperationRowDetail {
  index: number;
  orderId: string;
  status: string;
  message: string;
}

export interface ShippingRowDetail {
  index: number;
  orderNumber: string;
  trackingNumber: string;
  status: string;
  message: string;
}

export interface PaymentFileInfo {
  fileName: string;
  fileFullName: string;
  status: string;
  total: number;
  successCount: number;
  failedCount: number;
  shippingTotal: number;
  shippingSuccessCount: number;
  shippingFailedCount: number;
  operation?: string | null;
  shipping?: string | null;
  message?: string | null;
  operationDetails?: OperationRowDetail[] | null;
  shippingDetails?: ShippingRowDetail[] | null;
}

function statusClass(s: string): string {
  if (s === "Checked") return "status-pass";
  if (s === "Uploaded") return "status-successful";
  if (s === "Failed" || (s && s.toLowerCase().startsWith("error"))) return "status-failed status-error";
  return "";
}

export function PaymentUpload() {
  const [files, setFiles] = useState<PaymentFileInfo[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [expandedRowKeys, setExpandedRowKeys] = useState<string[]>([]);
  const [activeFileFullName, setActiveFileFullName] = useState<string | null>(null);
  const [activePhase, setActivePhase] = useState<"check" | "upload" | null>(null);

  const { Title, Text } = Typography;
  const { TextArea } = Input;

  const appendLog = (msg: string, newLine = false): void => {
    appendTimedLog(setLogs, msg, newLine);
  };

  const handleSelectFiles = async () => {
    const selected = await open({
      multiple: true,
      filters: [{ name: "Excel", extensions: ["xlsx", "xls"] }],
    });
    if (!selected) return;
    const filePaths = Array.isArray(selected) ? selected : [selected];
    setLoading(true);
    appendLog("Adding files…");
    try {
      const result = await invoke<PaymentFileInfo[]>("payment_add_files", {
        filePaths,
      });
      setFiles((prev) => {
        const existing = new Set(prev.map((f) => f.fileFullName.toLowerCase()));
        const added = result.filter((f) => !existing.has(f.fileFullName.toLowerCase()));
        return [...prev, ...added];
      });
      appendLog(`Selected ${filePaths.length} files.`, true);
    } catch (e) {
      appendLog(String(e), true);
      message.error(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleCheck = async () => {
    if (files.length === 0) {
      message.warning("Please select at least one Excel file.");
      return;
    }
    setLoading(true);
    appendLog("Starting check Excel files…");
    try {
      const fileNames = files.map((f) => f.fileName);
      const existingCustomers = await invoke<string[]>("payment_get_existing_customers", {
        fileNames,
      });

      const updated = [...files];
      for (let i = 0; i < updated.length; i++) {
        const file = updated[i];
        setActiveFileFullName(file.fileFullName);
        setActivePhase("check");
        try {
          await invoke("payment_validate_file", {
            filePath: file.fileFullName,
            fileName: file.fileName,
            existingCustomers,
          });
          appendLog(`${file.fileName}: Check passed.`);
          updated[i] = { ...file, status: "Checked", message: undefined };
        } catch (e) {
          appendLog(`${file.fileName}: Check failed - ${e}`, true);
          updated[i] = { ...file, status: "Failed", message: "file format wrong" };
        } finally {
          setFiles([...updated]);
          setActiveFileFullName(null);
          setActivePhase(null);
        }
      }

      setFiles(updated);
      appendLog("Check completed.", true);
    } catch (e) {
      appendLog(String(e), true);
      message.error(String(e));
    } finally {
      setLoading(false);
      setActiveFileFullName(null);
      setActivePhase(null);
    }
  };

  const handleUpload = async () => {
    const toUpload = files.filter((f) => f.status === "Checked");
    if (toUpload.length === 0) {
      message.warning("No files to upload (please execute Check first).");
      return;
    }
    const confirmed = await confirm(
      "Are you sure you want to upload all checked files? This operation cannot be undone.",
      { title: "Confirm Upload", kind: "warning" }
    );
    if (!confirmed) return;
    setLoading(true);
    appendLog("Starting upload…");
    try {
      const updated = [...files];
      for (let i = 0; i < toUpload.length; i++) {
        const file = toUpload[i];
        setActiveFileFullName(file.fileFullName);
        setActivePhase("upload");
        try {
          const result = await invoke<{
            opTotal: number;
            opUpdated: number;
            shipTotal: number;
            shipR1: number;
            shipR2: number;
            shipUnmatched: number;
            opDetails: OperationRowDetail[];
            shipDetails: ShippingRowDetail[];
          }>("payment_process_file", { filePath: file.fileFullName });

          const opFailed = result.opTotal - result.opUpdated;
          const shipSuccess = result.shipR1 + result.shipR2;
          const idx = updated.findIndex((f) => f.fileFullName === file.fileFullName);
          if (idx >= 0) {
            updated[idx] = {
              ...updated[idx],
              status: "Uploaded",
              total: result.opTotal,
              successCount: result.opUpdated,
              failedCount: opFailed,
              shippingTotal: result.shipTotal,
              shippingSuccessCount: shipSuccess,
              shippingFailedCount: result.shipUnmatched,
              operation: `${result.opTotal}/${result.opUpdated}/${opFailed}`,
              shipping: `${result.shipTotal}/${shipSuccess}/${result.shipUnmatched}`,
              message: undefined,
              operationDetails: result.opDetails ?? [],
              shippingDetails: result.shipDetails ?? [],
            };
          }
          if (idx >= 0) {
            appendLog(
              `${file.fileName}: Operation ${String(updated[idx].operation)}, Shipping ${String(
                updated[idx].shipping,
              )}`,
            );
            setFiles([...updated]);
          }
        } catch (e) {
          const idx = updated.findIndex((f) => f.fileFullName === file.fileFullName);
          if (idx >= 0) {
            updated[idx] = {
              ...updated[idx],
              status: "Failed",
              message: String(e),
            };
          }
          appendLog(`${file.fileName}: ${String(e)}`, true);
          if (idx >= 0) {
            setFiles([...updated]);
          }
        } finally {
          setActiveFileFullName(null);
          setActivePhase(null);
        }
      }
      setFiles(updated);
      appendLog("Upload completed.", true);
    } catch (e) {
      appendLog(String(e), true);
      message.error(String(e));
    } finally {
      setLoading(false);
      setActiveFileFullName(null);
      setActivePhase(null);
    }
  };

  const handleRemove = async () => {
    if (!selectedKey) {
      message.warning("Please select a row first.");
      return;
    }
    const confirmed = await confirm("Are you sure you want to delete the current row?", {
      title: "Delete Confirmation",
      kind: "warning",
    });
    if (!confirmed) return;
    setFiles((prev) => prev.filter((f) => f.fileFullName !== selectedKey));
    setSelectedKey(null);
  };

  const handleClear = async () => {
    const confirmed = await confirm("Are you sure you want to clear?", {
      title: "Clear Confirmation",
      kind: "warning",
    });
    if (!confirmed) return;
    setFiles([]);
    setLogs([]);
    setSelectedKey(null);
    setActiveFileFullName(null);
    setActivePhase(null);
    setExpandedRowKeys([]);
  };

  const columns: ColumnsType<PaymentFileInfo> = [
    {
      title: "Progress",
      key: "progress",
      width: 80,
      render: (_value, record) => {
        const isActive = loading && record.fileFullName === activeFileFullName;
        const done =
          record.status === "Checked" ||
          record.status === "Uploaded" ||
          record.status === "Failed";

        // check 阶段：Checked/Failed 代表已完成；upload 阶段：Uploaded/Failed 代表已完成
        const phaseDone =
          activePhase === "upload"
            ? record.status === "Uploaded" || record.status === "Failed"
            : activePhase === "check"
              ? record.status === "Checked" || record.status === "Failed"
              : done;

        const percent = phaseDone ? 100 : isActive ? 50 : 0;
        return (
          <Progress
            type="circle"
            percent={percent}
            width={28}
            strokeWidth={8}
            status={phaseDone ? "success" : isActive ? "active" : "normal"}
            format={() => ""}
          />
        );
      },
    },
    { title: "FileName", dataIndex: "fileName", ellipsis: true },
    { title: "Status", dataIndex: "status", render: (t: string) => <span className={statusClass(t)}>{t}</span> },
    { title: "Operation (Total/Success/Failed)", dataIndex: "operation" },
    { title: "Shipping (Total/Success/Failed)", dataIndex: "shipping" },
    { title: "Message", dataIndex: "message", ellipsis: true },
  ];

  const stepItems = [
    "1. Select Excel file(s) (must contain 3PL Operation Fee and 3PL Shipping sheets).",
    "2. Click \"Check\" to validate format and customer existence.",
    "3. Click \"Upload\" to write data to WMS.",
    "4. After upload, click the expand icon on the left of a row to view Operation/Shipping details.",
  ];

  const canOperate = files.length > 0;
  const anyChecked = files.some((f) => f.status === "Checked");

  return (
    <div className="payment-upload-page">
      <Space direction="vertical" size={4} style={{ width: "100%" }}>
        <Title level={3} style={{ marginBottom: 0 }}>
          Payment Upload Tools(Operation / Shipping Fee Paid)
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
                <Button
                  type="primary"
                  icon={<FileAddOutlined />}
                  onClick={handleSelectFiles}
                  disabled={loading}
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
                  disabled={!canOperate || loading || !anyChecked}
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
        <Table<PaymentFileInfo>
          columns={columns}
          dataSource={files}
          rowKey="fileFullName"
          pagination={false}
          size="small"
          rowClassName={(record) => (selectedKey === record.fileFullName ? "row-selected" : "")}
          onRow={(record) => ({ onClick: () => setSelectedKey(record.fileFullName) })}
          expandable={{
            expandedRowKeys,
            onExpandedRowsChange: (keys) => setExpandedRowKeys(keys as string[]),
            rowExpandable: (record) => record.status === "Uploaded",
            expandedRowRender: (record) => (
              <div className="payment-detail-expand">
                <Tabs
                  size="small"
                  defaultActiveKey="operation"
                  items={[
                    {
                      key: "operation",
                      label: `Operation (${record.operationDetails?.length ?? 0})`,
                      children: (
                        <Table<OperationRowDetail>
                          size="small"
                          dataSource={record.operationDetails ?? []}
                          rowKey="index"
                          columns={[
                            { title: "#", dataIndex: "index", width: 56 },
                            { title: "OrderId", dataIndex: "orderId", ellipsis: true },
                            {
                              title: "Status",
                              dataIndex: "status",
                              width: 90,
                              render: (t: string) => (
                                <span className={t === "Matched" ? "status-pass" : "status-failed"}>
                                  {t}
                                </span>
                              ),
                            },
                            { title: "Message", dataIndex: "message", ellipsis: true },
                          ]}
                          pagination={false}
                          scroll={{ y: 300 }}
                        />
                      ),
                    },
                    {
                      key: "shipping",
                      label: `Shipping (${record.shippingDetails?.length ?? 0})`,
                      children: (
                        <Table<ShippingRowDetail>
                          size="small"
                          dataSource={record.shippingDetails ?? []}
                          rowKey="index"
                          columns={[
                            { title: "#", dataIndex: "index", width: 56 },
                            { title: "OrderNumber", dataIndex: "orderNumber", ellipsis: true },
                            { title: "TrackingNumber", dataIndex: "trackingNumber", ellipsis: true },
                            {
                              title: "Status",
                              dataIndex: "status",
                              width: 90,
                              render: (t: string) => (
                                <span className={t === "Matched" ? "status-pass" : "status-failed"}>
                                  {t}
                                </span>
                              ),
                            },
                            { title: "Message", dataIndex: "message", ellipsis: true },
                          ]}
                          pagination={false}
                          scroll={{ y: 300 }}
                        />
                      ),
                    },
                  ]}
                />
              </div>
            ),
          }}
        />
      </Card>
    </div>
  );
}
