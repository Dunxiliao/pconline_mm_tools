import { useEffect, useState } from "react";
import { Button, Card, Form, Input, Modal, Popconfirm, Select, Space, Table, message } from "antd";
import { SearchOutlined, UserAddOutlined } from "@ant-design/icons";
import {
  listUsers,
  register,
  resetUserPassword,
  setUserStatus,
  type AuthUserItem,
} from "../services/authApi";

interface UserManagerProps {
  accessToken: string;
  merchantCode: string;
}

export function UserManager({ accessToken, merchantCode }: UserManagerProps) {
  const [registerLoading, setRegisterLoading] = useState(false);
  const [users, setUsers] = useState<AuthUserItem[]>([]);
  const [usersLoading, setUsersLoading] = useState(false);
  const [userPage, setUserPage] = useState(1);
  const [userPageSize, setUserPageSize] = useState(20);
  const [userTotal, setUserTotal] = useState(0);
  const [userNameFilter, setUserNameFilter] = useState("");
  const [isActiveFilter, setIsActiveFilter] = useState<boolean | undefined>(undefined);
  const [openUserModal, setOpenUserModal] = useState(false);
  const [resetPwdOpen, setResetPwdOpen] = useState(false);
  const [resetUserDisplay, setResetUserDisplay] = useState<{ userName: string; displayName?: string }>({
    userName: "",
    displayName: "",
  });

  const [registerForm] = Form.useForm<{
    account: string;
    password: string;
    displayName?: string;
    email?: string;
    phone?: string;
    remark?: string;
  }>();
  const [resetPwdForm] = Form.useForm<{ userId: string; password: string }>();

  const handleLoadUsers = async (nextPage = userPage, nextPageSize = userPageSize) => {
    if (!accessToken) return;
    setUsersLoading(true);
    try {
      const result = await listUsers(accessToken, {
        page: nextPage,
        pageSize: nextPageSize,
        merchantCode,
        userName: userNameFilter || undefined,
        isActive: isActiveFilter,
      });
      setUsers(result.items);
      setUserTotal(result.total);
      setUserPage(nextPage);
      setUserPageSize(nextPageSize);
    } catch (e) {
      message.error(String(e));
    } finally {
      setUsersLoading(false);
    }
  };

  const handleRegister = async (values: {
    account: string;
    password: string;
    displayName?: string;
    email?: string;
    phone?: string;
    remark?: string;
  }) => {
    setRegisterLoading(true);
    try {
      const result = await register(values);
      if (!result.success) {
        throw new Error(result.message || "Open user failed.");
      }
      message.success(result.message || "Open user successfully.");
      setOpenUserModal(false);
      registerForm.resetFields();
      await handleLoadUsers(1, userPageSize);
    } catch (e) {
      message.error(String(e));
    } finally {
      setRegisterLoading(false);
    }
  };

  const handleToggleUser = async (row: AuthUserItem, nextActive: boolean) => {
    try {
      await setUserStatus(accessToken, row.userId, nextActive);
      message.success(`User ${nextActive ? "enabled" : "disabled"} successfully.`);
      await handleLoadUsers();
    } catch (e) {
      message.error(String(e));
    }
  };

  const handleResetPassword = async () => {
    const values = await resetPwdForm.validateFields();
    try {
      await resetUserPassword(accessToken, values.userId, values.password);
      message.success("Password reset successfully.");
      setResetPwdOpen(false);
      resetPwdForm.resetFields();
    } catch (e) {
      message.error(String(e));
    }
  };

  useEffect(() => {
    handleLoadUsers(1, userPageSize);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accessToken, merchantCode]);

  return (
    <div style={{ flex: 1, minHeight: 0, overflow: "hidden", display: "flex", flexDirection: "column" }}>
      <Card size="small" style={{ flex: 1, minHeight: 0 }} bodyStyle={{ padding: 8, height: "100%" }}>
        <div
          style={{
            display: "flex",
            justifyContent: "flex-start",
            alignItems: "center",
            gap: 12,
            marginBottom: 12,
          }}
        >
          <Space wrap>
            <Input
              placeholder="Filter by user name"
              value={userNameFilter}
              onChange={(e) => setUserNameFilter(e.target.value)}
              style={{ width: 220 }}
            />
            <Select
              allowClear
              placeholder="Active status"
              value={isActiveFilter}
              onChange={(v) => setIsActiveFilter(v)}
              style={{ width: 140 }}
              options={[
                { label: "Active", value: true },
                { label: "Inactive", value: false },
              ]}
            />
            <Button icon={<SearchOutlined />} onClick={() => handleLoadUsers(1, userPageSize)}>
              Search
            </Button>
          </Space>
        </div>
        <div style={{ marginBottom: 12 }}>
          <Button
            type="primary"
            size="small"
            icon={<UserAddOutlined />}
            onClick={() => setOpenUserModal(true)}
          >
            Open User
          </Button>
        </div>
        <Table<AuthUserItem>
          size="small"
          rowKey={(r) => r.userId}
          loading={usersLoading}
          dataSource={users}
          scroll={{ y: 420 }}
          pagination={{
            current: userPage,
            pageSize: userPageSize,
            total: userTotal,
            showSizeChanger: true,
            onChange: (p, ps) => handleLoadUsers(p, ps),
          }}
          columns={[
            { title: "User", dataIndex: "userName", key: "userName", width: 120 },
            { title: "Display Name", dataIndex: "displayName", key: "displayName", width: 140 },
            { title: "Email", dataIndex: "email", key: "email", width: 180 },
            { title: "Phone", dataIndex: "phone", key: "phone", width: 120 },
            {
              title: "Status",
              key: "isActive",
              width: 80,
              render: (_v, row) => (row.isActive ? "Active" : "Inactive"),
            },
            {
              title: "Action",
              key: "action",
              width: 220,
              render: (_v, row) => (
                <Space size={4}>
                  <Popconfirm
                    title={row.isActive ? "Disable this user?" : "Enable this user?"}
                    onConfirm={() => handleToggleUser(row, !row.isActive)}
                  >
                    <Button size="small">{row.isActive ? "Disable" : "Enable"}</Button>
                  </Popconfirm>
                  <Button
                    size="small"
                    onClick={() => {
                      resetPwdForm.setFieldsValue({ userId: row.userId, password: "" });
                      setResetUserDisplay({
                        userName: row.userName,
                        displayName: row.displayName,
                      });
                      setResetPwdOpen(true);
                    }}
                  >
                    Reset Password
                  </Button>
                </Space>
              ),
            },
          ]}
        />
      </Card>

      <Modal
        title="Open User"
        open={openUserModal}
        onCancel={() => setOpenUserModal(false)}
        onOk={() => registerForm.submit()}
        okText="Open User"
        okButtonProps={{ loading: registerLoading }}
      >
        <Form form={registerForm} layout="vertical" onFinish={handleRegister}>
          <Form.Item label="Account" name="account" rules={[{ required: true, message: "Please input account" }]}>
            <Input placeholder="Enter account" autoComplete="off" />
          </Form.Item>
          <Form.Item label="Password" name="password" rules={[{ required: true, message: "Please input password" }]}>
            <Input.Password placeholder="Enter password" autoComplete="new-password" />
          </Form.Item>
          <Form.Item label="Display Name" name="displayName">
            <Input placeholder="Enter display name" autoComplete="off" />
          </Form.Item>
          <Form.Item
            label="Email"
            name="email"
            rules={[{ type: "email", message: "Please enter a valid email address." }]}
          >
            <Input placeholder="Enter email" autoComplete="off" />
          </Form.Item>
          <Form.Item label="Phone" name="phone">
            <Input placeholder="Enter phone" autoComplete="off" />
          </Form.Item>
          <Form.Item label="Remark" name="remark">
            <Input.TextArea placeholder="Enter remark" autoSize={{ minRows: 2, maxRows: 4 }} />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="Reset Password"
        open={resetPwdOpen}
        onCancel={() => setResetPwdOpen(false)}
        onOk={handleResetPassword}
      >
        <Form form={resetPwdForm} layout="vertical">
          <Form.Item name="userId" hidden>
            <Input />
          </Form.Item>
          <Form.Item label="User">
            <Input value={resetUserDisplay.userName} disabled />
          </Form.Item>
          <Form.Item label="Display Name">
            <Input value={resetUserDisplay.displayName || "—"} disabled />
          </Form.Item>
          <Form.Item label="New Password" name="password" rules={[{ required: true, message: "Please input new password" }]}>
            <Input.Password placeholder="Enter new password" autoComplete="new-password" />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
