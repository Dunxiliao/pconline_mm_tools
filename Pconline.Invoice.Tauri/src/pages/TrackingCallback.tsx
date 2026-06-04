import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, confirm, save } from "@tauri-apps/plugin-dialog";
import ExcelJS from "exceljs";
import {
  Table,
  Card,
  Row,
  Col,
  Space,
  Typography,
  Select,
  Checkbox,
  Button,
  Input,
  message,
  Tooltip,
  Badge,
} from "antd";
import {
  FileAddOutlined,
  DatabaseOutlined,
  ProfileOutlined,
  UploadOutlined,
  DeleteOutlined,
  ExportOutlined,
  ClearOutlined,
  DownloadOutlined,
} from "@ant-design/icons";
import type { ColumnsType } from "antd/es/table";
import { appendTimedLog } from "../utils/logging";

interface PlatformInfo {
  provider: number;
  providerName: string;
  seller: string;
}

interface TrackingPackageDto {
  sku: string;
  qty: number;
  trackingNumber: string;
}

interface TrackingOrderDto {
  orderNumber: string;
  trackingNumber: string;
  carrierName: string;
  serviceCode: string;
  status: string;
  message?: string;
  packages: TrackingPackageDto[];
}

interface TrackingPostItemResult {
  orderNumber: string;
  status: string;
  message?: string | null;
}

interface TrackingPostResult {
  success: boolean;
  message?: string | null;
  successOrderNumbers: string[];
  failedOrderNumbers: string[];
  data?: TrackingPostItemResult[];
}

interface TrackingCallbackProps {
  accessToken: string;
}

export function TrackingCallback({ accessToken }: TrackingCallbackProps) {
  const [platforms, setPlatforms] = useState<PlatformInfo[]>([]);
  const [platform, setPlatform] = useState<PlatformInfo | null>(null);
  const [orders, setOrders] = useState<TrackingOrderDto[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [merchantCode, setMerchantCode] = useState("pconline");
  const [replaceTracking, setReplaceTracking] = useState(false);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [isDataBase, setIsDataBase] = useState(false);
  const [shippingLabelLogName, setShippingLabelLogName] = useState("");
  const [orderNumberText, setOrderNumberText] = useState("");

  const { Title, Text } = Typography;
  const { TextArea } = Input;
  const getPlatformKey = (item: PlatformInfo): string => `${item.provider}::${item.seller}`;

  useEffect(() => {
    invoke<PlatformInfo[]>("platform_list").then(setPlatforms).catch(console.error);
    invoke<string>("get_etailflow_merchant_code").then(setMerchantCode).catch(() => {});
  }, []);

  const appendLog = (msg: string, newLine = false): void => {
    appendTimedLog(setLogs, msg, newLine);
  };

  const handleDownloadTemplate = async () => {
    const path = await save({
      defaultPath: "Tracking Callback.xlsx",
      filters: [{ name: "Excel", extensions: ["xlsx"] }],
    });
    if (!path) return;
    setLoading(true);
    try {
      await invoke("tracking_download_template", { path });
      appendLog("Template saved.", true);
      message.success("Template saved.");
    } catch (e) {
      const msg = String(e);
      appendLog(msg, true);
      message.error(msg);
    } finally {
      setLoading(false);
    }
  };

  const handleSelectFiles = async () => {
    const selected = await open({
      filters: [{ name: "Excel", extensions: ["xlsx", "xls"] }],
    });
    if (!selected) return;
    const filePaths = Array.isArray(selected) ? selected.slice(0, 1) : [selected];
    setLoading(true);
    setIsDataBase(false);
    appendLog("Loading and parsing Excel");
    try {
      const result = await invoke<TrackingOrderDto[]>("tracking_parse_excel", { filePaths });
      setOrders((prev) => {
        if (prev.length === 0) return result;

        const byOrder = new Map<string, TrackingOrderDto>();
        for (const o of prev) {
          byOrder.set(o.orderNumber, { ...o, packages: [...o.packages] });
        }

        for (const next of result) {
          const exist = byOrder.get(next.orderNumber);
          if (!exist) {
            byOrder.set(next.orderNumber, next);
            continue;
          }
          
          const mergedPackages: TrackingPackageDto[] = [...exist.packages, ...next.packages];
          const mergedTrackingNumbers = Array.from(new Set(mergedPackages.map((p) => p.trackingNumber))).join(",");

          byOrder.set(next.orderNumber, {
            ...exist,
            // 若重新解析同一订单，更新运单字段；上传成功/失败状态保持不变（跟随现有行）
            carrierName: next.carrierName || exist.carrierName,
            serviceCode: next.serviceCode || exist.serviceCode,
            trackingNumber: mergedTrackingNumbers,
            packages: mergedPackages,
          });
        }

        return Array.from(byOrder.values());
      });

      appendLog(`Parsed ${result.length} order(s).`, true);
    } catch (e) {
      const msg = String(e);
      appendLog(msg, true);
      message.error(msg);
    } finally {
      setLoading(false);
    }
  };

  const handleUpload = async () => {
    if (orders.length === 0) {
      message.warning("No orders to upload.");
      return;
    }
    if (!platform) {
      message.warning("Please select a platform.");
      return;
    }
    const confirmed = await confirm(
      "Are you sure you want to upload tracking numbers? ",
      { title: "Upload", kind: "warning" }
    );
    if (!confirmed) return;
    setLoading(true);
    appendLog("Begin upload data, Please keep the window open.");
    try {
      const isMultiTracking = (trackingNumber: string): boolean => {
        const parts = trackingNumber
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean);
        return parts.length > 1;
      };

      // 所有平台都批量提交：
      // - Successful 的行：跳过，不改状态
      // - 若来自数据库且 trackingNumber 包含多个逗号值：跳过，并记录日志
      const currentOrders = orders.map((o) => {
        if (o.status === "Successful") return o;
        if (isDataBase && isMultiTracking(o.trackingNumber)) return o; // 跳过：不重置状态
        // 批量上传：等待后端返回期间标记为 Uploading
        return { ...o, status: "Uploading" };
      });

      setOrders([...currentOrders]);

      const allOrders = currentOrders.map((o) => o.orderNumber);
      const skippedMultiTrackingFromDbOrderNumbers = currentOrders
        .filter((o) => isDataBase && o.status !== "Successful" && isMultiTracking(o.trackingNumber))
        .map((o) => o.orderNumber);

      const ordersToUpload = currentOrders.filter((o) => o.status === "Uploading");

      if (ordersToUpload.length === 0) {
        appendLog("Completed upload data!", true);
        return;
      }

      const result = await invoke<TrackingPostResult>("tracking_post_etailflow", {
        accessToken,
        merchantCode,
        serviceProvider: platform.provider,
        seller: platform.seller,
        orders: ordersToUpload,
        replaceTracking,
      });

      const {
        success,
        message: msg,
        successOrderNumbers,
        failedOrderNumbers: fOrderNumbers,
        data,
      } = result;
      const lastMsg = msg ?? undefined;
      const itemResultMap = new Map<string, TrackingPostItemResult>(
        (data ?? []).map((item) => [item.orderNumber, item]),
      );

      const failedOrderNumbers: string[] = [];
      if (success) {
        const successSet = new Set(successOrderNumbers);
        const failedSet = new Set(fOrderNumbers);
        for (const o of ordersToUpload) {
          o.status = successSet.has(o.orderNumber) ? "Successful" : failedSet.has(o.orderNumber) ? "Failed" : "Failed";
          const itemResult = itemResultMap.get(o.orderNumber);
          o.message = o.status === "Failed" && itemResult?.message ? itemResult.message : undefined;
          if (o.status === "Failed") failedOrderNumbers.push(o.orderNumber);
        }
      } else {
        for (const o of ordersToUpload) {
          o.status = "Failed";
          const itemResult = itemResultMap.get(o.orderNumber);
          o.message = itemResult?.message ?? undefined;
          failedOrderNumbers.push(o.orderNumber);
        }
      }

      setOrders([...currentOrders]);

      if (!success) {
        appendLog(`OrderNumber: ${allOrders.join(", ")} upload failed! ${lastMsg ?? ""}`.trim(), true);
      } else if (failedOrderNumbers.length) {
        // appendLog(`Failed orders: ${Array.from(new Set(failedOrderNumbers)).join(", ")}`);
        appendLog("Completed upload data!", true);
      } else {
        appendLog("Completed upload data!", true);
      }

      if (isDataBase && skippedMultiTrackingFromDbOrderNumbers.length > 0) {
        appendLog(
          `The datasource from database,currently does not support uploading multiple tracking numbers. The order numbers:${Array.from(
            new Set(skippedMultiTrackingFromDbOrderNumbers),
          ).join(",")}`,
          true,
        );
      }
    } catch (e) {
      const msg = String(e);
      const allOrders = orders.map((o) => o.orderNumber).join(", ");
      appendLog(
        `OrderNumber: ${allOrders} upload failed! ${msg}`.trim(),
        true,
      );
      // 上传接口整体失败（如网络错误）时，将当前仍未标记状态的行统一标记为 Failed
      setOrders((prev) =>
        prev.map((o) => {
          if (o.status === "Successful") return o;
          if (isDataBase && o.status !== "Successful" && o.trackingNumber.split(",").map((s) => s.trim()).filter(Boolean).length > 1) return o;
          return { ...o, status: "Failed" };
        }),
      );
    } finally {
      setLoading(false);
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
    setOrders((prev) => prev.filter((o) => o.orderNumber !== selectedKey));
    setSelectedKey(null);
  };

  const handleClear = async () => {
    const confirmed = await confirm("Are you sure you want to clear?", {
      title: "Clear",
      kind: "warning",
    });
    if (!confirmed) return;
    setOrders([]);
    setLogs([]);
    setPlatform(null);
    setSelectedKey(null);
    setIsDataBase(false);
    setShippingLabelLogName("");
    setOrderNumberText("");
  };

  const handlePullDatabase = async () => {
    if (!platform) {
      message.warning("Please select a platform first.");
      return;
    }
    setIsDataBase(true);
    setLoading(true);
    appendLog("Pull Database…");
    try {
      const result = await invoke<TrackingOrderDto[]>("tracking_pull_database", {
        provider: platform.provider,
      });
      setOrders(result);
      appendLog(`Loaded ${result.length} order(s) from database.`, true);
    } catch (e) {
      appendLog(String(e), true);
    } finally {
      setLoading(false);
    }
  };

  const handleAssignOrder = async () => {
    if (!platform) {
      message.warning("Please select a platform first.");
      return;
    }
    setIsDataBase(false);
    const label = shippingLabelLogName.trim();
    const lines = orderNumberText.trim().split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
    if (label && lines.length > 0) {
      message.warning("Order number and Shipping Label Log cannot be specified at the same time.");
      return;
    }
    if (!label && lines.length === 0) {
      message.warning("Please enter Shipping Label Log Name or Order Number.");
      return;
    }
    setLoading(true);
    appendLog("Assign Order…");
    try {
      const result = await invoke<TrackingOrderDto[]>("tracking_assign_order", {
        provider: platform.provider,
        shippingLabelLogName: label || undefined,
        orderNumberLines: lines.length > 0 ? orderNumberText.trim() : undefined,
      });
      setOrders(result);
      if (result.length === 0) {
        appendLog("Not Match Order Number", true);
      } else {
        appendLog(`Loaded ${result.length} order(s).`, true);
      }
    } catch (e) {
      appendLog(String(e), true);
    } finally {
      setLoading(false);
    }
  };

  const handleExport = async () => {
    if (orders.length === 0) {
      message.warning("No data to export.");
      return;
    }
    const path = await save({
      defaultPath: "Tracking Callback Export.xlsx",
      filters: [{ name: "Excel", extensions: ["xlsx"] }],
    });
    if (!path) return;
    setLoading(true);
    try {
      const workbook = new ExcelJS.Workbook();
      const worksheet = workbook.addWorksheet("Tracking Callback");

      worksheet.addRow([
        "OrderNumber",
        "TrackingNumber",
        "CarrierName",
        "ServiceCode",
        "Sku",
        "Qty",
      ]);

      for (const o of orders) {
        for (const p of o.packages) {
          worksheet.addRow([
            o.orderNumber,
            o.trackingNumber,
            o.carrierName,
            o.serviceCode,
            p.sku,
            p.qty,
          ]);
        }
      }

      const xlsxBuffer = await workbook.xlsx.writeBuffer();
      const anyBuffer: any = xlsxBuffer;
      const u8 =
        anyBuffer instanceof ArrayBuffer
          ? new Uint8Array(anyBuffer)
          : anyBuffer instanceof Uint8Array
            ? anyBuffer
            : new Uint8Array(anyBuffer.buffer);

      // base64 encode for invoke
      let binary = "";
      const chunkSize = 0x8000;
      for (let i = 0; i < u8.length; i += chunkSize) {
        const chunk = u8.subarray(i, i + chunkSize);
        for (let j = 0; j < chunk.length; j++) {
          binary += String.fromCharCode(chunk[j]);
        }
      }
      const base64Xlsx = btoa(binary);

      await invoke("tracking_write_xlsx", { path, base64Xlsx });
      appendLog("Export Successful!", true);
    } catch (e) {
      appendLog(String(e), true);
    } finally {
      setLoading(false);
    }
  };

  function statusClass(s: string): string {
    if (s === "Successful") return "status-successful";
    if (s === "Failed") return "status-failed status-error";
    if (s === "Uploading") return "status-uploading";
    return "";
  }

  const columns: ColumnsType<TrackingOrderDto> = [
    {
      title: "#",
      dataIndex: "index",
      width: 30,
      render: (_value, _record, index) => index + 1,
    },
    {
      title: "OrderNumber",
      dataIndex: "orderNumber",
      width: 120,
      ellipsis: true,
    },
    {
      title: "Tracking Number",
      dataIndex: "trackingNumber",
      width: 120,
      ellipsis: true,
      render: (value: string) => {
        const numbers = Array.from(
          new Set(
            String(value ?? "")
              .split(",")
              .map((s) => s.trim())
              .filter(Boolean),
          ),
        );
        if (numbers.length === 0) return "—";
        if (numbers.length === 1) return numbers[0];
        const extraCount = numbers.length - 1;
        return (
          <Tooltip
            title={
              <Space direction="vertical" size={0}>
                {numbers.map((n) => (
                  <Text key={n} style={{ color: "#fff" }}>
                    {n}
                  </Text>
                ))}
              </Space>
            }
          >
            <Space size={6}>
              <span>{numbers[0]}</span>
              <Badge count={`+${extraCount}`} />
            </Space>
          </Tooltip>
        );
      },
    },
    {
      title: "CarrierName",
      dataIndex: "carrierName",
      width: 100,
      ellipsis: true,
    },
    {
      title: "ServiceCode",
      dataIndex: "serviceCode",
      width: 120,
      ellipsis: true,
    },
    {
      title: "Status",
      dataIndex: "status",
      width: 60,
      render: (text: string) => <span className={statusClass(text)}>{text || "—"}</span>,
    },
    {
      title: "Message",
      dataIndex: "message",
      width: 120,
      render: (text: string) => <Text type="danger">{text}</Text>,
    }
  ];

  return (
    <div className="tracking-callback">
      <Space direction="vertical" size={4} style={{ width: "100%" }}>
        <Title level={3} style={{ marginBottom: 0 }}>
          PCO Tracking CallBack Tools
        </Title>
      </Space>
      <Row gutter={16} style={{ flex: "0 0 auto" }}>
        <Col flex="1 1 0" style={{ minWidth: 0 }}>
          <Card
            size="small"
            style={{ borderRadius: 12 }}
            bodyStyle={{ padding: 16 }}
          >
            <Space direction="vertical" size="small" style={{ width: "100%" }}>
              <Text>Steps: 1. Select the Excel file or Pull Database or Assign Order. 2. Click Upload.</Text>
              <Text type="warning">Notes: Avoid duplicate uploads!</Text>
              <Space align="center" wrap>
                <Text strong>E-commerce platform:</Text>
                <Select
                  style={{ minWidth: 220 }}
                  placeholder="Select..."
                  value={platform ? getPlatformKey(platform) : undefined}
                  onChange={(value) => {
                    const p = platforms.find((x) => getPlatformKey(x) === value);
                    setPlatform(p ?? null);
                  }}
                  options={platforms.map((p) => ({
                    value: getPlatformKey(p),
                    label: p.providerName,
                  }))}
                />
              </Space>
              <Space align="center" wrap>
                <Checkbox
                  checked={replaceTracking}
                  onChange={(e) => setReplaceTracking(e.target.checked)}
                >
                  Replace Tracking
                </Checkbox>
                <Text type="warning">Only Walmart orders within 4 hours are supported.</Text>
              </Space>
              <Space wrap>
                <Button
                  icon={<DownloadOutlined />}
                  onClick={handleDownloadTemplate}
                  loading={loading}
                >
                  Template
                </Button>
                <Button
                  type="primary"
                  icon={<FileAddOutlined />}
                  onClick={handleSelectFiles}
                  loading={loading}
                >
                  Selected Files
                </Button>
                <Button
                  icon={<DatabaseOutlined />}
                  onClick={handlePullDatabase}
                  loading={loading}
                  disabled={loading || !platform}
                >
                  Pull Database
                </Button>
                <Button
                  icon={<ProfileOutlined />}
                  onClick={handleAssignOrder}
                  loading={loading}
                  disabled={loading || !platform}
                >
                  Assign Order
                </Button>
              </Space>
              <Space wrap>
                <Button
                  type="primary"
                  icon={<UploadOutlined />}
                  onClick={handleUpload}
                  disabled={orders.length === 0 || loading || !platform}
                >
                  Upload
                </Button>
                <Button
                  icon={<DeleteOutlined />}
                  onClick={handleRemove}
                  disabled={orders.length === 0 || loading || !selectedKey}
                >
                  Remove
                </Button>
                <Button
                  icon={<ExportOutlined />}
                  onClick={handleExport}
                  disabled={orders.length === 0 || loading}
                >
                  Export
                </Button>
                <Button
                  danger
                  icon={<ClearOutlined />}
                  onClick={handleClear}
                  disabled={loading && orders.length === 0}
                >
                  Clear
                </Button>
              </Space>
            </Space>
          </Card>
        </Col>
        <Col flex="320px">
          <Card size="small" bodyStyle={{ padding: 8 }}>
            <Space direction="vertical" size="small" style={{ width: "100%" }}>
              <Text>Shipping Label Log Name:</Text>
              <Input
                value={shippingLabelLogName}
                onChange={(e) => setShippingLabelLogName(e.target.value)}
              />
              <Text>Order Number:</Text>
              <TextArea
                rows={6}
                value={orderNumberText}
                onChange={(e) => setOrderNumberText(e.target.value)}
              />
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

      <Card
        style={{ flex: "1 1 auto", minHeight: 0, display: "flex", flexDirection: "column" }}
        bodyStyle={{ padding: 0, flex: "1 1 auto", minHeight: 0, display: "flex", flexDirection: "column" }}
      >
        <div className="tracking-table-wrapper">
          <Table<TrackingOrderDto>
            columns={columns}
            dataSource={orders}
            rowKey={(record) => record.orderNumber}
            pagination={false}
            size="small"
            rowClassName={(record) => (selectedKey === record.orderNumber ? "row-selected" : "")}
            onRow={(record) => ({
              onClick: () => setSelectedKey(record.orderNumber),
            })}
            summary={() => (
              <Table.Summary>
                <Table.Summary.Row>
                  <Table.Summary.Cell index={0} colSpan={columns.length} align="right">
                    Total Records: {orders.length}
                  </Table.Summary.Cell>
                </Table.Summary.Row>
              </Table.Summary>
            )}
          />
        </div>
      </Card>
    </div>
  );
}
