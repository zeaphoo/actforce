use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    English,
    Chinese,
}

impl Language {
    pub fn from_code(code: &str) -> Self {
        match code.to_lowercase().as_str() {
            "zh" | "zh-cn" | "zh_cn" | "chinese" => Language::Chinese,
            _ => Language::English,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Chinese => "zh-CN",
        }
    }
}

pub fn get_system_language() -> Language {
    if let Ok(lang) = std::env::var("LANG") {
        let lang_code = lang.split('.').next().unwrap_or("en");
        if lang_code.starts_with("zh") {
            return Language::Chinese;
        }
    }
    Language::English
}

static TRANSLATIONS: OnceLock<Translations> = OnceLock::new();

pub fn init() {
    let mut translations = Translations::new();

    // English translations
    translations.add(Language::English, "installing", "Installing ActForce");
    translations.add(Language::English, "installation_dir", "Installation directory");
    translations.add(Language::English, "extracting_package", "Extracting package");
    translations.add(Language::English, "package_extracted", "Package extracted successfully");
    translations.add(Language::English, "backing_up", "Backing up existing installation");
    translations.add(Language::English, "backed_up_to", "Backed up to");
    translations.add(Language::English, "moving_files", "Moving files to installation directory");
    translations.add(Language::English, "loading_images", "Loading Docker images");
    translations.add(Language::English, "images_dir_not_found", "Images directory not found");
    translations.add(Language::English, "starting_services", "Starting ActForce services");
    translations.add(Language::English, "installation_completed", "ActForce installation completed successfully!");
    translations.add(Language::English, "usage_instructions", "ActForce Usage Instructions");
    translations.add(Language::English, "start_command", "Start command");
    translations.add(Language::English, "stop_command", "Stop command");
    translations.add(Language::English, "access_url", "Access URL");
    translations.add(Language::English, "upgrading", "Upgrading ActForce");
    translations.add(Language::English, "dir_not_found", "ActForce is not installed in the specified directory");
    translations.add(Language::English, "backing_up_config", "Backing up configuration files");
    translations.add(Language::English, "stopping_services", "Stopping current ActForce services");
    translations.add(Language::English, "existing_installation_detected", "Existing ActForce installation detected!");
    translations.add(Language::English, "status", "Status:");
    translations.add(Language::English, "installation_options", "Please select an option:");
    translations.add(Language::English, "upgrade", "Upgrade existing installation");
    translations.add(Language::English, "recommended", "recommended");
    translations.add(Language::English, "fresh_install", "Fresh installation");
    translations.add(Language::English, "will_stop_services", "will stop running services");
    translations.add(Language::English, "cancel", "Cancel");
    translations.add(Language::English, "abort_installation", "abort installation");
    translations.add(Language::English, "enter_choice", "Enter your choice");
    translations.add(Language::English, "proceeding_with_upgrade", "Proceeding with upgrade...");
    translations.add(Language::English, "proceeding_with_fresh_install", "Proceeding with fresh installation...");
    translations.add(Language::English, "stopping_existing_services", "Stopping existing services...");
    translations.add(Language::English, "failed_to_stop_services", "Failed to stop services");
    translations.add(Language::English, "continue_anyway", "Continue anyway?");
    translations.add(Language::English, "installation_aborted", "Installation aborted");
    translations.add(Language::English, "services_stopped", "Services stopped successfully");
    translations.add(Language::English, "extracting_new_version", "Extracting new version package");
    translations.add(Language::English, "migrating_config", "Migrating configuration files");
    translations.add(Language::English, "migrating_env_file", "Migrating .env file");
    translations.add(Language::English, "failed_read_old_env", "Failed to read old .env file");
    translations.add(Language::English, "failed_read_env_example", "Failed to read .env.example file");
    translations.add(Language::English, "adding_new_env_var", "Adding new environment variable");
    translations.add(Language::English, "failed_write_merged_env", "Failed to write merged .env file");
    translations.add(Language::English, "merged_config_success", "Successfully merged old configuration with new environment variables");
    translations.add(Language::English, "copied_old_env", "Copied old .env file to new installation");
    translations.add(Language::English, "config_migration_completed", "Configuration migration completed");
    translations.add(Language::English, "check_manual_config", "Please check for any manual configuration needed in the .env file");
    translations.add(Language::English, "no_backup_env_found", "No backup .env file found, using default configuration");
    translations.add(Language::English, "created_new_env", "Created new .env file from .env.example");
    translations.add(Language::English, "env_example_not_found", ".env.example file not found");
    translations.add(Language::English, "backing_up_and_deploying", "Backing up old version and deploying new version");
    translations.add(Language::English, "backed_up_old_version", "Backed up old version to");
    translations.add(Language::English, "deployed_new_version", "Deployed new version to");
    translations.add(Language::English, "copying_env_file", "Copying .env file from backup");
    translations.add(Language::English, "env_file_copied_successfully", "Environment file copied successfully");
    translations.add(Language::English, "no_env_backup_found", "No .env backup file found, please check your configuration");
    translations.add(Language::English, "starting_new_services", "Starting new ActForce services");
    translations.add(Language::English, "checking_service_status", "Checking ActForce service status");
    translations.add(Language::English, "upgrade_completed", "ActForce upgrade completed successfully!");
    translations.add(Language::English, "rollback_info", "If you need to rollback, use the backup directory");
    translations.add(Language::English, "loading_new_images", "Loading new Docker images");
    translations.add(Language::English, "removing", "Removing ActForce");

    // Docker related
    translations.add(Language::English, "checking_env_file", "Checking for .env file in");
    translations.add(Language::English, "env_file_found", ".env file found");
    translations.add(Language::English, "env_example_found", ".env.example file found");
    translations.add(Language::English, "creating_env_file", "Creating .env file from .env.example");
    translations.add(Language::English, "env_file_created", "Successfully created .env file");
    translations.add(Language::English, "failed_create_env", "Failed to create .env file");
    translations.add(Language::English, "no_env_files", "No .env or .env.example file found in");
    translations.add(Language::English, "services_may_not_start", "Services may not start correctly without environment configuration");
    translations.add(Language::English, "docker_dir_not_found", "Docker directory not found");
    translations.add(Language::English, "loading_images_from", "Loading Docker images from");
    translations.add(Language::English, "loading_image", "Loading image");
    translations.add(Language::English, "no_image_files", "No image files found in");
    translations.add(Language::English, "found_image_files", "Found Docker image files");
    translations.add(Language::English, "loaded_images", "Successfully loaded Docker images");
    translations.add(Language::English, "service_started_successfully", "ActForce service started successfully");
    translations.add(Language::English, "service_stopped_successfully", "ActForce service stopped successfully");
    translations.add(Language::English, "starting_actforce_service", "Starting ActForce service");
    translations.add(Language::English, "stopping_actforce_service", "Stopping ActForce service");

    // Image loading
    translations.add(Language::English, "loading_summary", "Loading summary:");
    translations.add(Language::English, "failed_images", "Failed images:");
    translations.add(Language::English, "select_images_to_load", "Select images to load:");
    translations.add(Language::English, "load_all_images", "Load all images");
    translations.add(Language::English, "quit", "Quit");
    translations.add(Language::English, "enter_options", "Enter options (multiple options separated by space, e.g.: 1 3)");
    translations.add(Language::English, "operation_cancelled", "Operation cancelled");
    translations.add(Language::English, "could_not_extract_image_name", "Could not extract image name from output, using latest loaded image ID");
    translations.add(Language::English, "no_target_image_specified", "No target image name specified, skipping tagging");
    translations.add(Language::English, "loading_images_manually", "Loading images manually");

    // Command descriptions
    translations.add(Language::English, "cmd_install", "Install ActForce");
    translations.add(Language::English, "cmd_upgrade", "Upgrade an existing ActForce installation");
    translations.add(Language::English, "arg_package_path", "Path to the ActForce package file");
    translations.add(Language::English, "arg_install_dir", "Installation directory");
    translations.add(Language::English, "arg_language", "Set language");
    translations.add(Language::English, "arg_verbose", "Enable verbose logging");

    // Upgrade/fresh install prompt
    translations.add(Language::English, "upgrade_or_fresh", "Please choose an action:");

    // Chinese translations
    translations.add(Language::Chinese, "installing", "正在安装 ActForce");
    translations.add(Language::Chinese, "installation_dir", "安装目录");
    translations.add(Language::Chinese, "extracting_package", "正在解压安装包");
    translations.add(Language::Chinese, "package_extracted", "安装包解压成功");
    translations.add(Language::Chinese, "backing_up", "正在备份现有安装");
    translations.add(Language::Chinese, "backed_up_to", "已备份到");
    translations.add(Language::Chinese, "moving_files", "正在移动文件到安装目录");
    translations.add(Language::Chinese, "loading_images", "正在加载 Docker 镜像");
    translations.add(Language::Chinese, "images_dir_not_found", "未找到镜像目录");
    translations.add(Language::Chinese, "starting_services", "正在启动 ActForce 服务");
    translations.add(Language::Chinese, "installation_completed", "ActForce 安装成功完成！");
    translations.add(Language::Chinese, "usage_instructions", "ActForce 使用说明");
    translations.add(Language::Chinese, "start_command", "启动命令");
    translations.add(Language::Chinese, "stop_command", "停止命令");
    translations.add(Language::Chinese, "access_url", "访问地址");
    translations.add(Language::Chinese, "upgrading", "正在升级 ActForce");
    translations.add(Language::Chinese, "dir_not_found", "指定目录中未安装 ActForce");
    translations.add(Language::Chinese, "backing_up_config", "正在备份配置文件");
    translations.add(Language::Chinese, "stopping_services", "正在停止当前 ActForce 服务");
    translations.add(Language::Chinese, "existing_installation_detected", "检测到现有 ActForce 安装！");
    translations.add(Language::Chinese, "status", "状态：");
    translations.add(Language::Chinese, "installation_options", "请选择一个选项：");
    translations.add(Language::Chinese, "upgrade", "升级现有安装");
    translations.add(Language::Chinese, "recommended", "推荐");
    translations.add(Language::Chinese, "fresh_install", "全新安装");
    translations.add(Language::Chinese, "will_stop_services", "将停止运行中的服务");
    translations.add(Language::Chinese, "cancel", "取消");
    translations.add(Language::Chinese, "abort_installation", "中止安装");
    translations.add(Language::Chinese, "enter_choice", "输入您的选择");
    translations.add(Language::Chinese, "proceeding_with_upgrade", "正在进行升级...");
    translations.add(Language::Chinese, "proceeding_with_fresh_install", "正在进行全新安装...");
    translations.add(Language::Chinese, "stopping_existing_services", "正在停止现有服务...");
    translations.add(Language::Chinese, "failed_to_stop_services", "停止服务失败");
    translations.add(Language::Chinese, "continue_anyway", "是否仍然继续？");
    translations.add(Language::Chinese, "installation_aborted", "安装已中止");
    translations.add(Language::Chinese, "services_stopped", "服务已成功停止");
    translations.add(Language::Chinese, "extracting_new_version", "正在解压新版本安装包");
    translations.add(Language::Chinese, "migrating_config", "正在迁移配置文件");
    translations.add(Language::Chinese, "migrating_env_file", "正在迁移 .env 文件");
    translations.add(Language::Chinese, "failed_read_old_env", "读取旧 .env 文件失败");
    translations.add(Language::Chinese, "failed_read_env_example", "读取 .env.example 文件失败");
    translations.add(Language::Chinese, "adding_new_env_var", "添加新环境变量");
    translations.add(Language::Chinese, "failed_write_merged_env", "写入合并后的 .env 文件失败");
    translations.add(Language::Chinese, "merged_config_success", "成功将旧配置与新环境变量合并");
    translations.add(Language::Chinese, "copied_old_env", "已将旧 .env 文件复制到新安装");
    translations.add(Language::Chinese, "config_migration_completed", "配置迁移完成");
    translations.add(Language::Chinese, "check_manual_config", "请检查 .env 文件中是否需要手动配置");
    translations.add(Language::Chinese, "no_backup_env_found", "未找到备份的 .env 文件，使用默认配置");
    translations.add(Language::Chinese, "created_new_env", "从 .env.example 创建了新的 .env 文件");
    translations.add(Language::Chinese, "env_example_not_found", "未找到 .env.example 文件");
    translations.add(Language::Chinese, "backing_up_and_deploying", "正在备份旧版本并部署新版本");
    translations.add(Language::Chinese, "backed_up_old_version", "已备份旧版本到");
    translations.add(Language::Chinese, "deployed_new_version", "已部署新版本到");
    translations.add(Language::Chinese, "copying_env_file", "从备份复制 .env 文件");
    translations.add(Language::Chinese, "env_file_copied_successfully", "环境配置文件复制成功");
    translations.add(Language::Chinese, "no_env_backup_found", "未找到 .env 备份文件，请检查您的配置");
    translations.add(Language::Chinese, "starting_new_services", "启动新的 ActForce 服务");
    translations.add(Language::Chinese, "checking_service_status", "检查 ActForce 服务状态");
    translations.add(Language::Chinese, "upgrade_completed", "ActForce 升级成功完成！");
    translations.add(Language::Chinese, "rollback_info", "如需回滚，请使用备份目录");
    translations.add(Language::Chinese, "loading_new_images", "正在加载新 Docker 镜像");
    translations.add(Language::Chinese, "removing", "正在移除 ActForce");

    // Docker related
    translations.add(Language::Chinese, "checking_env_file", "正在检查 .env 文件在");
    translations.add(Language::Chinese, "env_file_found", "找到 .env 文件");
    translations.add(Language::Chinese, "env_example_found", "找到 .env.example 文件");
    translations.add(Language::Chinese, "creating_env_file", "正在从 .env.example 创建 .env 文件");
    translations.add(Language::Chinese, "env_file_created", "成功创建 .env 文件");
    translations.add(Language::Chinese, "failed_create_env", "创建 .env 文件失败");
    translations.add(Language::Chinese, "no_env_files", "在以下目录中未找到 .env 或 .env.example 文件");
    translations.add(Language::Chinese, "services_may_not_start", "没有环境配置，服务可能无法正确启动");
    translations.add(Language::Chinese, "docker_dir_not_found", "未找到 Docker 目录");
    translations.add(Language::Chinese, "loading_images_from", "正在从以下位置加载 Docker 镜像");
    translations.add(Language::Chinese, "loading_image", "正在加载镜像");
    translations.add(Language::Chinese, "no_image_files", "在以下位置未找到镜像文件");
    translations.add(Language::Chinese, "found_image_files", "发现 Docker 镜像文件");
    translations.add(Language::Chinese, "loaded_images", "成功加载 Docker 镜像");
    translations.add(Language::Chinese, "service_started_successfully", "ActForce 服务启动成功");
    translations.add(Language::Chinese, "service_stopped_successfully", "ActForce 服务停止成功");
    translations.add(Language::Chinese, "starting_actforce_service", "正在启动 ActForce 服务");
    translations.add(Language::Chinese, "stopping_actforce_service", "正在停止 ActForce 服务");

    // Image loading
    translations.add(Language::Chinese, "loading_summary", "加载总结：");
    translations.add(Language::Chinese, "failed_images", "失败的镜像：");
    translations.add(Language::Chinese, "select_images_to_load", "选择要加载的镜像：");
    translations.add(Language::Chinese, "load_all_images", "加载所有镜像");
    translations.add(Language::Chinese, "quit", "退出");
    translations.add(Language::Chinese, "enter_options", "输入选项（多个选项用空格分隔，例如：1 3）");
    translations.add(Language::Chinese, "operation_cancelled", "操作已取消");
    translations.add(Language::Chinese, "could_not_extract_image_name", "无法从输出中提取镜像名称，使用最近加载的镜像 ID");
    translations.add(Language::Chinese, "no_target_image_specified", "未指定目标镜像名称，跳过标记");
    translations.add(Language::Chinese, "loading_images_manually", "手动加载镜像");

    // Command descriptions
    translations.add(Language::Chinese, "cmd_install", "安装 ActForce");
    translations.add(Language::Chinese, "cmd_upgrade", "升级现有的 ActForce 安装");
    translations.add(Language::Chinese, "arg_package_path", "ActForce 安装包路径");
    translations.add(Language::Chinese, "arg_install_dir", "安装目录");
    translations.add(Language::Chinese, "arg_language", "设置语言");
    translations.add(Language::Chinese, "arg_verbose", "启用详细日志输出");

    // Upgrade/fresh install prompt
    translations.add(Language::Chinese, "upgrade_or_fresh", "请选择操作：");

    TRANSLATIONS.set(translations).ok();
}

pub fn set_language(lang: Language) {
    if let Some(translations) = TRANSLATIONS.get() {
        let mut current = translations.current_language.write().unwrap();
        *current = lang;
    }
}

pub fn t(key: &str) -> String {
    if let Some(translations) = TRANSLATIONS.get() {
        translations.get(key)
    } else {
        key.to_string()
    }
}

struct Translations {
    translations: HashMap<Language, HashMap<String, String>>,
    current_language: RwLock<Language>,
}

impl Translations {
    fn new() -> Self {
        Self {
            translations: HashMap::new(),
            current_language: RwLock::new(get_system_language()),
        }
    }

    fn add(&mut self, lang: Language, key: &str, value: &str) {
        let entry = self.translations.entry(lang).or_insert_with(HashMap::new);
        entry.insert(key.to_string(), value.to_string());
    }

    fn get(&self, key: &str) -> String {
        let current = self.current_language.read().unwrap();

        if let Some(lang_translations) = self.translations.get(&*current) {
            if let Some(value) = lang_translations.get(key) {
                return value.clone();
            }
        }

        if let Some(en_translations) = self.translations.get(&Language::English) {
            if let Some(value) = en_translations.get(key) {
                return value.clone();
            }
        }

        key.to_string()
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}
