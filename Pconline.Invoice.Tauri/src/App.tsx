import "./App.css";
import { useState, useEffect, useRef } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Layout, Menu, Typography, Card, Space, Row, Col, ConfigProvider, theme, Modal, message, Button, Breadcrumb } from "antd";
import { HomeOutlined, CloudUploadOutlined, UploadOutlined, DollarOutlined, UserAddOutlined, HistoryOutlined } from "@ant-design/icons";
import { TrackingCallback } from "./pages/TrackingCallback";
import { InvoiceUpload } from "./pages/InvoiceUpload";
import { PaymentUpload } from "./pages/PaymentUpload";
import { Login } from "./pages/Login";
import { UserManager } from "./pages/UserManager";
import { ReleaseHistory } from "./pages/ReleaseHistory";
import {
  getRuntimeConfig,
  login,
  type RuntimeConfig,
} from "./services/authApi";

type Page = "home" | "tracking" | "invoice" | "payment" | "user" | "releases";

const { Sider, Content, Footer } = Layout;
const { Title, Paragraph } = Typography;

function App() {
  const [page, setPage] = useState<Page>("home");
  const [version, setVersion] = useState<string>("");
  const [isAuthed, setIsAuthed] = useState(false);
  const [loginLoading, setLoginLoading] = useState(false);
  const [runtimeConfig, setRuntimeConfig] = useState<RuntimeConfig | null>(null);
  const [tokenExpireAt, setTokenExpireAt] = useState<number | null>(null);
  const [accessToken, setAccessToken] = useState("");
  const [currentUser, setCurrentUser] = useState<string>("");
  const expireModalOpenedRef = useRef(false);

  // Update-related state (kept for easy re-enable)
  const [updateModalOpen, setUpdateModalOpen] = useState(false);
  const [pendingUpdate, setPendingUpdate] = useState<Awaited<ReturnType<typeof check>> | null>(null);
  const [updateDownloading, setUpdateDownloading] = useState(false);
  const checkedUpdateAfterLoginRef = useRef(false);
  const getUpdaterHeaders = () =>
    accessToken
      ? {
          Authorization: `Bearer ${accessToken}`,
        }
      : undefined;

  // Call backend get_app_version, fallback to package version
  useEffect(() => {
    const apply = (v: string) => setVersion(typeof v === "string" && v.trim() ? v : "—");
    invoke<string>("get_app_version")
      .then(apply)
      .catch(() => getVersion().then(apply).catch(() => apply("")));
  }, []);

  const handleLogout = () => {
    setIsAuthed(false);
    setRuntimeConfig(null);
    setTokenExpireAt(null);
    setCurrentUser("");
    setAccessToken("");
    expireModalOpenedRef.current = false;
    checkedUpdateAfterLoginRef.current = false;
  };

  const handleLogin = async (values: { username: string; password: string }) => {
    setLoginLoading(true);
    try {
      const auth = await login(values);
      const config = await getRuntimeConfig(auth.accessToken);
      const merchantCodeFromEnv = await invoke<string>("get_etailflow_merchant_code").catch(() => "");
      const mergedConfig: RuntimeConfig = {
        ...config,
        merchantCode: config.merchantCode?.trim() || merchantCodeFromEnv.trim() || "",
      };
      await invoke("apply_runtime_config", { config: mergedConfig });
      setRuntimeConfig(mergedConfig);
      setAccessToken(auth.accessToken);
      setCurrentUser(values.username.trim());
      setIsAuthed(true);
      setTokenExpireAt(auth.expiresAt ?? null);
      expireModalOpenedRef.current = false;
      message.success("Login successful.");
    } catch (e) {
      message.error(String(e));
      throw e;
    } finally {
      setLoginLoading(false);
    }
  };


  const handleCheckUpdate = async (silent = false) => {
    try {
      const update = await check({
        headers: getUpdaterHeaders(),
      });
      if (update) {
        setPendingUpdate(update);
        setUpdateModalOpen(true);
      } else if (!silent) {
        message.info("You are already on the latest version.");
      }
    } catch (e) {
      const err = String(e).toLowerCase();
      if (err.includes("401") || err.includes("403") || err.includes("unauthorized")) {
        if (!silent) {
          message.error("Your login is invalid or expired. Please sign in again and retry.");
        }
        return;
      }
      if (!silent) {
        message.error("Check update failed: " + String(e));
      }
    }
  };

  useEffect(() => {
    if (!isAuthed) return;
    if (checkedUpdateAfterLoginRef.current) return;
    checkedUpdateAfterLoginRef.current = true;
    void handleCheckUpdate(true);
  }, [isAuthed]);

  const handleInstallUpdate = async () => {
    if (!pendingUpdate) return;
    setUpdateDownloading(true);
    try {
      await pendingUpdate.downloadAndInstall(undefined, {
        headers: getUpdaterHeaders(),
      });
      setUpdateModalOpen(false);
      setPendingUpdate(null);
      await relaunch();
    } catch (e) {
      const err = String(e).toLowerCase();
      if (err.includes("401") || err.includes("403") || err.includes("unauthorized")) {
        setUpdateDownloading(false);
        message.error("Your login is invalid or expired. Please sign in again before updating.");
        return;
      }
      setUpdateDownloading(false);
      message.error("Install update failed: " + String(e));
    }
  };

  const menuItems = [
    { key: "home", icon: <HomeOutlined />, label: "Home" },
    { key: "tracking", icon: <CloudUploadOutlined />, label: "PCO Tracking CallBack Tools" },
    { key: "invoice", icon: <UploadOutlined />, label: "PCO Invoice Upload Tools" },
    { key: "payment", icon: <DollarOutlined />, label: "3PL Payment Upload Tools" },
    { key: "user", icon: <UserAddOutlined />, label: "User Manager" },
    { key: "releases", icon: <HistoryOutlined />, label: "Release history" },
  ];
  const pageLabelMap: Record<Page, string> = {
    home: "Home",
    tracking: "PCO Tracking CallBack Tools",
    invoice: "PCO Invoice Upload Tools",
    payment: "3PL Payment Upload Tools",
    user: "User Manager",
    releases: "Release history",
  };

  useEffect(() => {
    if (!isAuthed || !tokenExpireAt) return;

    const delay = tokenExpireAt - Date.now();
    if (delay <= 0) {
      if (!expireModalOpenedRef.current) {
        expireModalOpenedRef.current = true;
        Modal.warning({
          title: "Login Expired",
          content: "Your session has expired. Please sign in again.",
          okText: "OK",
          onOk: () => {
            handleLogout();
          },
        });
      }
      return;
    }

    const timer = window.setTimeout(() => {
      if (expireModalOpenedRef.current) return;
      expireModalOpenedRef.current = true;
      Modal.warning({
        title: "Login Expired",
        content: "Your session has expired. Please sign in again.",
        okText: "OK",
        onOk: () => {
          handleLogout();
        },
      });
    }, delay);

    return () => window.clearTimeout(timer);
  }, [isAuthed, tokenExpireAt]);

  if (!isAuthed) {
    return (
      <ConfigProvider
        theme={{
          algorithm: theme.darkAlgorithm,
          token: {
            colorBgLayout: "#020617",
            colorBgContainer: "#020617",
            colorText: "#e5e7eb",
            colorTextSecondary: "#9ca3af",
            colorPrimary: "#3b82f6",
            borderRadius: 8,
          },
        }}
      >
        <Login
          loginLoading={loginLoading}
          version={version}
          onLogin={handleLogin}
        />
      </ConfigProvider>
    );
  }

  return (
    <ConfigProvider
      theme={{
        algorithm: theme.darkAlgorithm,
        token: {
          colorBgLayout: "#020617",
          colorBgContainer: "#020617",
          colorText: "#e5e7eb",
          colorTextSecondary: "#9ca3af",
          colorPrimary: "#3b82f6",
          borderRadius: 8,
        },
      }}
    >
      <Layout style={{ height: "100vh", overflow: "hidden" }}>
        <Sider theme="dark" width={240}>
          <div style={{ padding: "16px 16px 8px" }}>
            <Title level={4} style={{ color: "#fff", margin: 0 }}>
              Pconline Toolset
            </Title>
          </div>
          <Menu
            theme="dark"
            mode="inline"
            selectedKeys={[page]}
            onClick={(info) => setPage(info.key as Page)}
            items={menuItems}
          />
        </Sider>
        <Layout>
          <Content
            style={{
              padding: "16px 16px 0",
              height: "100%",
              overflow: "hidden",
              display: "flex",
              flexDirection: "column",
            }}
          >
            <div style={{ display: "flex", justifyContent: "flex-end", marginBottom: 8 }}>
              <Space size={8}>
                <Typography.Text type="secondary">User: {currentUser || "—"}</Typography.Text>
                <Typography.Text type="secondary">Merchant: {runtimeConfig?.merchantCode || "—"}</Typography.Text>
                <Button
                  size="small"
                  type="default"
                  style={{
                    minWidth: 68,
                    color: "#dbe4f0",
                    background: "rgba(15, 23, 42, 0.22)",
                    borderColor: "rgba(148, 163, 184, 0.35)",
                    fontWeight: 500,
                  }}
                  onClick={() => {
                    handleLogout();
                    message.success("Signed out.");
                  }}
                >
                  Logout
                </Button>
              </Space>
            </div>
            <div style={{ marginBottom: 10 }}>
              <Breadcrumb
                items={[
                  { title: "Home" },
                  ...(page === "home" ? [] : [{ title: pageLabelMap[page] }]),
                ]}
              />
            </div>
            {page === "home" && (
              <div style={{ flex: 1, minHeight: 0, minWidth: 0, overflow: "auto", overflowX: "hidden", width: "100%" }}>
                <Space direction="vertical" size="middle" style={{ width: "100%", display: "block" }}>
                  <Title level={3} style={{ marginBottom: 8 }}>
                    Welcome to Pconline Toolset
                  </Title>
                  <Row gutter={[16, 16]}>
                    <Col xs={24} md={12}>
                      <Card title="PCO Tracking CallBack Tools" size="small">
                        <Paragraph style={{ marginBottom: 8 }}>
                          Tools for handling tracking number callbacks and related operations.
                        </Paragraph>
                        <Paragraph strong>Supported Platforms:</Paragraph>
                        <ul style={{ marginBottom: 0 }}>
                          <li>Walmart</li>
                          <li>Newegg</li>
                          <li>Newegg Business</li>
                          <li>Goflow</li>
                          <li>Shipstation</li>
                          <li>Shein mmm</li>
                        </ul>
                      </Card>
                    </Col>
                    <Col xs={24} md={12}>
                      <Card title="PCO Invoice Upload Tools" size="small">
                        <Paragraph style={{ marginBottom: 8 }}>
                          Tools for uploading and processing invoice files.
                        </Paragraph>
                        <Paragraph strong>Supported Carriers:</Paragraph>
                        <ul style={{ marginBottom: 0 }}>
                          <li>Amazon Shipping</li>
                          <li>UPS</li>
                          <li>Fedex</li>
                          <li>Temu</li>
                        </ul>
                      </Card>
                    </Col>
                    <Col xs={24} md={12}>
                      <Card title="3PL Payment Upload Tools" size="small">
                        <Paragraph style={{ marginBottom: 8 }}>
                          Tools for uploading 3PL client payment files to mark corresponding orders and packages as paid on WMS.
                        </Paragraph>
                        <ul style={{ marginBottom: 0 }}>
                          <li>3PL Operation Fee</li>
                          <li>3PL Shipping Fee</li>
                        </ul>
                      </Card>
                    </Col>
                  </Row>
                  <Card title="Instructions for using the tools" size="small" style={{ marginTop: 16 }}>
                    <ol style={{ marginBottom: 0 }}>
                      <li>Select the tool you need from the left menu.</li>
                      <li>Follow the prompts on the tool interface.</li>
                      <li>Check the log output if you encounter any issues.</li>
                    </ol>
                  </Card>
                </Space>
              </div>
            )}
            {page === "tracking" && <TrackingCallback accessToken={accessToken} />}
            {page === "invoice" && <InvoiceUpload />}
            {page === "payment" && <PaymentUpload />}
            {page === "user" && (
              <UserManager
                accessToken={accessToken}
                merchantCode={runtimeConfig?.merchantCode || ""}
              />
            )}
            {page === "releases" && <ReleaseHistory />}
          </Content>
          <Footer
            style={{
              textAlign: "center",
              padding: "8px 16px 12px",
              marginTop: 4,
              color: "rgba(229, 231, 235, 0.72)",
            }}
          >
            <Space size={10} style={{ justifyContent: "center", width: "100%" }}>
              <Typography.Text type="secondary">
                {version
                  ? version.startsWith("Version ")
                    ? version
                    : `Version ${version}`
                  : "Version —"}
              </Typography.Text>
              <Typography.Text type="secondary">|</Typography.Text>
              <Button
                type="link"
                size="small"
                style={{
                  height: "auto",
                  padding: 0,
                  color: "rgba(147, 197, 253, 0.95)",
                  fontWeight: 500,
                }}
                onClick={() => {
                  void handleCheckUpdate(false);
                }}
              >
                Check Update
              </Button>
            </Space>

            <Modal
              title="New Version Found"
              open={updateModalOpen}
              onCancel={() => {
                setUpdateModalOpen(false);
                setPendingUpdate(null);
              }}
              onOk={handleInstallUpdate}
              okText={updateDownloading ? "Installing…" : "Install Now"}
              okButtonProps={{ loading: updateDownloading }}
              cancelButtonProps={{ disabled: updateDownloading }}
            >
              {pendingUpdate && (
                <>
                  <p>Version: {pendingUpdate.version}</p>
                  {pendingUpdate.body && <p>{pendingUpdate.body}</p>}
                </>
              )}
            </Modal>
          </Footer>
        </Layout>
      </Layout>
    </ConfigProvider>
  );
}

export default App;
