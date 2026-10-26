const { app, BrowserWindow, BrowserView, Menu, ipcMain } = require("electron");
const path = require("path");
const fs = require("fs");

// Load app config from environment variable or CLI argument
const CONFIG_ENV = process.env.APP_CONFIG || process.argv.find(a => a.startsWith("--config="))?.split("=")[1] || "actforce";
const CONFIGS_DIR = path.join(__dirname, "..", "configs");
const configPath = path.join(CONFIGS_DIR, `${CONFIG_ENV}.json`);

let appConfig = {
  id: "actforce",
  name: "ActForce",
  productName: "ActForce",
  defaultUrl: "http://localhost:6173",
  theme: {
    backgroundColor: "#1a1a1a",
    titlebarColor: "#1a1a1a",
    textColor: "#e5e5e5",
    borderColor: "#333",
  },
  window: {
    width: 1200,
    height: 800,
    minWidth: 800,
    minHeight: 600,
  },
};

if (fs.existsSync(configPath)) {
  appConfig = { ...appConfig, ...JSON.parse(fs.readFileSync(configPath, "utf-8")) };
}

const WEB_APP_URL = process.env.WEB_APP_URL || appConfig.defaultUrl;
const TITLEBAR_HEIGHT = 42;

let mainWindow;
let browserView;

function createWindow() {
  // 创建主窗口（无边框）
  mainWindow = new BrowserWindow({
    width: appConfig.window.width,
    height: appConfig.window.height,
    minWidth: appConfig.window.minWidth,
    minHeight: appConfig.window.minHeight,
    frame: false,
    backgroundColor: appConfig.theme.backgroundColor,
    title: appConfig.productName,
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false,
    },
  });

  // 加载标题栏 HTML 到主窗口
  mainWindow.loadFile(path.join(__dirname, "titlebar-overlay.html"));

  // 创建 BrowserView 用于加载网页
  browserView = new BrowserView({
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
    },
  });

  mainWindow.setBrowserView(browserView);

  // 加载网页到 BrowserView
  browserView.webContents.loadURL(WEB_APP_URL);

  // 设置 BrowserView 的位置和大小
  setContentBounds();

  // 窗口大小变化时调整 BrowserView
  mainWindow.on("resize", setContentBounds);

  // 开发环境打开开发者工具
  if (process.env.NODE_ENV === "development") {
    browserView.webContents.openDevTools();
  }

  // 暴露应用配置给标题栏
  ipcMain.handle("get-app-config", () => appConfig);
}

function setContentBounds() {
  if (!mainWindow || !browserView) return;

  const { width, height } = mainWindow.getBounds();
  browserView.setBounds({
    x: 1,
    y: TITLEBAR_HEIGHT + 1,
    width: width - 2,
    height: height - TITLEBAR_HEIGHT - 2,
  });
}

app.whenReady().then(() => {
  Menu.setApplicationMenu(null);
  createWindow();
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});

app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0) {
    createWindow();
  }
});

// 窗口控制 IPC
ipcMain.on("window-minimize", () => {
  mainWindow.minimize();
});

ipcMain.on("window-maximize", () => {
  if (mainWindow.isMaximized()) {
    mainWindow.unmaximize();
  } else {
    mainWindow.maximize();
  }
});

ipcMain.on("window-close", () => {
  mainWindow.close();
});
