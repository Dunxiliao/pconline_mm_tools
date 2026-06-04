import { invoke } from "@tauri-apps/api/core";

export interface LoginRequest {
  username: string;
  password: string;
}

export interface RuntimeConfig {
  invoiceDbUrl?: string;
  odooDbUrl?: string;
  efshipDbUrl?: string;
  wmsDbUrl?: string;
  etailflowUrl: string;
  merchantCode: string;
  envName: "test" | "prod";
  features: {
    trackingUpload: boolean;
    invoiceUpload: boolean;
    paymentUpload: boolean;
  };
}

export interface LoginResponse {
  accessToken: string;
  refreshToken?: string;
  expiresIn: number;
  expiresAt?: number;
}

export interface RegisterResponse {
  success: boolean;
  message?: string;
}

export interface RegisterRequest {
  account: string;
  password: string;
  displayName?: string;
  email?: string;
  phone?: string;
  remark?: string;
}

export interface UserListQuery {
  page: number;
  pageSize: number;
  merchantCode: string;
  userName?: string;
  isActive?: boolean;
}

export interface AuthUserItem {
  userId: string;
  userName: string;
  displayName?: string;
  email?: string;
  phone?: string;
  isActive: boolean;
}

export interface UserListResult {
  items: AuthUserItem[];
  total: number;
}

function normalizeUserItem(row: Record<string, any>): AuthUserItem {
  return {
    userId: String(row.user_id ?? row.userId ?? row.id ?? ""),
    userName: String(row.user_name ?? row.userName ?? row.account ?? ""),
    displayName: row.display_name ?? row.displayName ?? undefined,
    email: row.email ?? undefined,
    phone: row.phone ?? undefined,
    isActive: Boolean(row.is_active ?? row.isActive ?? false),
  };
}

function parseJwtExpireAt(token: string): number | undefined {
  const parts = token.split(".");
  if (parts.length < 2) return undefined;
  try {
    const base64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const padded = base64 + "=".repeat((4 - (base64.length % 4)) % 4);
    const json = atob(padded);
    const payload = JSON.parse(json) as { exp?: number };
    if (typeof payload.exp !== "number" || payload.exp <= 0) return undefined;
    return payload.exp * 1000;
  } catch {
    return undefined;
  }
}

function resolveExpiresAt(accessToken: string, expiresIn: number): number | undefined {
  const jwtExpiresAt = parseJwtExpireAt(accessToken);
  if (jwtExpiresAt) return jwtExpiresAt;
  if (expiresIn <= 0) return undefined;
  // Some APIs return absolute unix timestamp (seconds), others return ttl seconds.
  if (expiresIn > 1_000_000_000) return expiresIn * 1000;
  return Date.now() + expiresIn * 1000;
}

export async function login(payload: LoginRequest): Promise<LoginResponse> {
  const username = payload.username.trim();
  const password = payload.password.trim();
  if (!username || !password) {
    throw new Error("Username and password are required.");
  }
  const data = await invoke<{
    accessToken: string;
    refreshToken?: string;
    expiresIn: number;
  }>("auth_login", {
    payload: {
      account: username,
      password,
    },
  });
  const accessToken = data?.accessToken;
  if (!accessToken) {
    throw new Error("Login response missing access token.");
  }
  const expiresIn = Number(data?.expiresIn) || 0;
  return {
    accessToken,
    refreshToken: data?.refreshToken,
    expiresIn,
    expiresAt: resolveExpiresAt(accessToken, expiresIn),
  };
}

export async function getRuntimeConfig(accessToken: string): Promise<RuntimeConfig> {
  const config = await invoke<Record<string, any>>("auth_get_runtime_config", {
    accessToken,
  });
  return {
    invoiceDbUrl: config?.invoice_db_url ?? config?.invoiceDbUrl,
    odooDbUrl: config?.odoo_db_url ?? config?.odooDbUrl,
    efshipDbUrl: config?.efship_db_url ?? config?.efshipDbUrl,
    wmsDbUrl: config?.wms_db_url ?? config?.wmsDbUrl,
    etailflowUrl: String(config?.etailflow_url ?? config?.etailflowUrl ?? ""),
    merchantCode: String(config?.merchant_code ?? config?.merchantCode ?? ""),
    envName: String(config?.env_name ?? config?.envName ?? "prod").toLowerCase() === "test" ? "test" : "prod",
    features: {
      trackingUpload: Boolean(config?.tracking_upload ?? config?.trackingUpload ?? true),
      invoiceUpload: Boolean(config?.invoice_upload ?? config?.invoiceUpload ?? true),
      paymentUpload: Boolean(config?.payment_upload ?? config?.paymentUpload ?? true),
    },
  };
}

export async function register(payload: RegisterRequest): Promise<RegisterResponse> {
  const account = payload.account.trim();
  const password = payload.password.trim();
  if (!account || !password) {
    throw new Error("Account and password are required.");
  }

  const data = await invoke<{
    success?: boolean;
    message?: string;
  }>("auth_register", {
    payload: {
      account,
      password,
      displayName: payload.displayName?.trim() || undefined,
      email: payload.email?.trim() || undefined,
      phone: payload.phone?.trim() || undefined,
      remark: payload.remark?.trim() || undefined,
    },
  });

  return {
    success: data?.success ?? true,
    message: data?.message,
  };
}

export async function listUsers(accessToken: string, query: UserListQuery): Promise<UserListResult> {
  const data = await invoke<Record<string, any>>("auth_list_users", {
    accessToken,
    page: query.page,
    pageSize: query.pageSize,
    merchantCode: query.merchantCode,
    userName: query.userName?.trim() || undefined,
    isActive: query.isActive,
  });

  const rows = (data?.items ??
    data?.list ??
    data?.rows ??
    data?.records ??
    data?.data ??
    []) as Record<string, any>[];
  const total = Number(data?.total ?? data?.count ?? data?.total_count ?? rows.length) || rows.length;
  return {
    items: Array.isArray(rows) ? rows.map((r) => normalizeUserItem(r)) : [],
    total,
  };
}

export async function setUserStatus(accessToken: string, userId: string, isActive: boolean): Promise<void> {
  await invoke("auth_set_user_status", {
    accessToken,
    userId,
    isActive,
  });
}

export async function resetUserPassword(accessToken: string, userId: string, password: string): Promise<void> {
  await invoke("auth_reset_user_password", {
    accessToken,
    userId,
    password,
  });
}

export async function refreshAccessToken(refreshToken: string): Promise<LoginResponse> {
  const data = await invoke<{
    accessToken: string;
    refreshToken?: string;
    expiresIn: number;
  }>("auth_refresh", {
    refreshToken,
  });
  const accessToken = data?.accessToken;
  if (!accessToken) {
    throw new Error("Refresh response missing access token.");
  }
  const expiresIn = Number(data?.expiresIn) || 0;
  return {
    accessToken,
    refreshToken: data?.refreshToken ?? refreshToken,
    expiresIn,
    expiresAt: resolveExpiresAt(accessToken, expiresIn),
  };
}
