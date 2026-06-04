import { useEffect } from "react";
import { Button, Card, Checkbox, Form, Input, Space, Typography } from "antd";

interface LoginFormValues {
  username: string;
  password: string;
  remember?: boolean;
}

const STORAGE_KEYS = {
  remember: "pconline.login.remember",
  username: "pconline.login.username",
  password: "pconline.login.password",
} as const;

function loadSavedCredentials(): Partial<LoginFormValues> {
  if (localStorage.getItem(STORAGE_KEYS.remember) !== "1") {
    return { remember: false };
  }
  return {
    remember: true,
    username: localStorage.getItem(STORAGE_KEYS.username) ?? "",
    password: localStorage.getItem(STORAGE_KEYS.password) ?? "",
  };
}

function persistCredentials(values: LoginFormValues) {
  if (values.remember) {
    localStorage.setItem(STORAGE_KEYS.remember, "1");
    localStorage.setItem(STORAGE_KEYS.username, values.username.trim());
    localStorage.setItem(STORAGE_KEYS.password, values.password);
  } else {
    localStorage.removeItem(STORAGE_KEYS.remember);
    localStorage.removeItem(STORAGE_KEYS.username);
    localStorage.removeItem(STORAGE_KEYS.password);
  }
}

interface LoginProps {
  loginLoading?: boolean;
  version?: string;
  onLogin: (values: LoginFormValues) => Promise<void>;
}

export function Login({ loginLoading = false, version = "", onLogin }: LoginProps) {
  const [loginForm] = Form.useForm<LoginFormValues>();
  const { Title, Text } = Typography;

  useEffect(() => {
    loginForm.setFieldsValue(loadSavedCredentials());
  }, [loginForm]);

  const handleFinish = async (values: LoginFormValues) => {
    await onLogin({ username: values.username, password: values.password });
    persistCredentials(values);
  };

  return (
    <div style={{ minHeight: "100vh", display: "flex", alignItems: "center", justifyContent: "center", padding: 24 }}>
      <Card style={{ width: 420 }} bodyStyle={{ padding: 24 }}>
        <Space direction="vertical" size={8} style={{ width: "100%" }}>
          <Title level={3} style={{ marginBottom: 0 }}>
            Pconline Toolset Login
          </Title>
          <Text type="secondary">Please sign in with your account and password.</Text>
        </Space>
        <Form<LoginFormValues>
          form={loginForm}
          layout="vertical"
          style={{ marginTop: 12 }}
          onFinish={handleFinish}
        >
          <Form.Item label="Username" name="username" rules={[{ required: true, message: "Please input username" }]}>
            <Input placeholder="Enter username" autoComplete="username" />
          </Form.Item>
          <Form.Item label="Password" name="password" rules={[{ required: true, message: "Please input password" }]}>
            <Input.Password placeholder="Enter password" autoComplete="current-password" />
          </Form.Item>
          <Form.Item name="remember" valuePropName="checked" style={{ marginBottom: 12 }}>
            <Checkbox>Remember username and password</Checkbox>
          </Form.Item>
          <Button type="primary" htmlType="submit" loading={loginLoading} block>
            Sign In
          </Button>
        </Form>
        <div style={{ marginTop: 12, textAlign: "center" }}>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {version ? (version.startsWith("Version ") ? version : `Version ${version}`) : ""}
          </Text>
        </div>
      </Card>
    </div>
  );
}
