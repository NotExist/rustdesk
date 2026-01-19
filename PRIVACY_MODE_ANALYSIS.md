# RustDesk 隱私模式實作分析與比較

## 執行摘要

本文件分析 RustDesk 當前的隱私模式實作方式，評估其優缺點，並與其他主流遠端操作工具（TeamViewer、AnyDesk、Chrome Remote Desktop）進行比較。

---

## 一、RustDesk 隱私模式實作方式

### 1.1 架構概覽

**平台支援：** 僅支援 Windows（macOS 和 Linux 為空實作）

**核心機制：** 提供三種不同的實作策略，根據系統版本和硬體支援自動選擇

**實作檔案結構：**
```
src/privacy_mode.rs                          # 核心模組和策略選擇
src/privacy_mode/win_exclude_from_capture.rs # 策略 1：視窗排除
src/privacy_mode/win_mag.rs                  # 策略 2：放大鏡 API
src/privacy_mode/win_topmost_window.rs       # 策略 2 輔助：最上層黑色視窗
src/privacy_mode/win_virtual_display.rs      # 策略 3：虛擬顯示器
src/privacy_mode/win_input.rs                # 輸入攔截（鍵盤/滑鼠）
```

### 1.2 三種實作策略詳解

#### 策略 1：視窗排除法 (Exclude from Capture)

**檔案位置：** `win_exclude_from_capture.rs`

**系統需求：** Windows 10 Build 19041 (版本 2004) 或更高

**技術原理：**
```
1. 使用 Windows API: SetWindowDisplayAffinity()
2. 設定排除標誌的視窗
3. Windows Magnification API 自動排除此視窗
4. 遠端用戶看到的畫面為黑屏
```

**優點：**
- ✅ 實作最簡單、程式碼最少
- ✅ 效能開銷最小（<100ms）
- ✅ 同步執行，立即生效
- ✅ 不需要額外的驅動程式或服務
- ✅ 實體顯示器保持正常運作
- ✅ 最穩定，不會影響顯示器配置

**缺點：**
- ❌ 僅支援 Windows 10 2004 或更高版本
- ❌ 實體螢幕仍然顯示內容（只是遠端看不到）
- ❌ 本地用戶可以看到正在進行的操作
- ❌ 隱私保護較弱（適合防止遠端截圖，不適合完全隱私）

**實作評估：**
這是 RustDesk 的**首選策略**，當系統支援時會優先使用。適合不需要完全隱藏本地螢幕的場景。

---

#### 策略 2：放大鏡視窗法 (Magnifier + Topmost Window)

**檔案位置：** `win_mag.rs`, `win_topmost_window.rs`

**系統需求：** Windows 10 或更高

**技術原理：**
```
1. 啟動 RuntimeBroker_rustdesk.exe（suspended 狀態）
2. 注入 WindowInjection.dll 至該程序
3. 建立名為 "RustDeskPrivacyWindow" 的黑色最上層視窗
4. 使用 Windows Magnification API：
   - 建立放大鏡視窗
   - 呼叫 MagSetWindowFilterList() 設定 MW_FILTERMODE_EXCLUDE
   - 排除隱私視窗不被擷取
5. 顯示視窗覆蓋整個螢幕
```

**DLL 注入流程：**
```rust
CreateProcessAsUserW()         // 以當前用戶身分建立程序（suspended）
  ↓
LoadLibraryW()                 // 注入 WindowInjection.dll
  ↓
ResumeThread()                 // 恢復執行緒
  ↓
黑色視窗覆蓋螢幕 + 放大鏡 API 排除
```

**優點：**
- ✅ 實體螢幕被黑色視窗完全覆蓋
- ✅ 本地用戶無法看到遠端操作
- ✅ 同步執行，快速啟動（通常 <100ms）
- ✅ 相容性較好（支援 Windows 10+）
- ✅ 不需要虛擬顯示器驅動

**缺點：**
- ❌ 實作複雜（需要 DLL 注入和程序管理）
- ❌ 需要額外的 RuntimeBroker_rustdesk.exe 和 WindowInjection.dll
- ❌ 安全軟體可能將 DLL 注入視為惡意行為
- ❌ 程序崩潰可能導致隱私模式失效
- ❌ 需要驗證放大鏡擷取器是否正確排除視窗（需執行測試）
- ❌ Alt+Tab 可能繞過（雖然已用 hook 攔截 Alt 鍵）
- ❌ 依賴 Windows Magnification API 穩定性

**實作評估：**
這是**次選策略**，當策略 1 不支援時使用。提供較好的隱私保護，但實作複雜度和安全風險較高。

---

#### 策略 3：虛擬顯示器法 (Virtual Display)

**檔案位置：** `win_virtual_display.rs`

**系統需求：**
- Windows 10 Build 19041+
- 必須安裝為系統服務
- 需要虛擬顯示器驅動程式

**支援的驅動程式：**
- `IDD_IMPL_AMYUNI` - Amyuni/USB Mobile Monitor 驅動
- `IDD_IMPL_RUSTDESK` - RustDesk 自己的 IDD 驅動

**技術原理（四階段流程）：**

**階段 1 - 準備實體顯示器：**
```rust
1. 枚舉所有當前活動的實體顯示器
2. 儲存它們的當前顯示設定（DEVMODEW 結構）
3. 記錄主顯示器標誌
```

**階段 2 - 建立虛擬顯示器：**
```rust
1. 使用 IDD 驅動程式插入虛擬顯示器
2. 預設解析度：1920x1080@60Hz
3. 等待最多 6 秒讓虛擬顯示器出現
4. 驗證虛擬顯示器已就緒
```

**階段 3 - 切換至虛擬顯示器：**
```rust
1. 將虛擬顯示器設為主顯示器（位置 0,0）
2. 停用所有實體顯示器：
   - 移動到座標 (10000, 10000)
   - 設定解析度為 0x0（停用）
3. 使用 ChangeDisplaySettingsExW() 套用設定
4. 儲存登錄檔連接設定（用於恢復）
```

**階段 4 - 恢復（關閉隱私模式時）：**
```rust
1. 恢復原始顯示器設定
2. 拔除虛擬顯示器
3. 恢復登錄檔顯示器連接設定
4. 使用 ChangeDisplaySettingsExW(CDS_RESET) 強制重新載入
```

**非同步執行機制：**
```rust
// 在 privacy_mode.rs:213-229
async fn turn_on_privacy_async() {
    std::thread::spawn(|| {
        turn_on_privacy_sync()
    });
    // 最多等待 7.5 秒
    timeout(7500, rx).await
}
```

**優點：**
- ✅ 實體顯示器被完全停用（解析度設為 0x0）
- ✅ 最強的隱私保護（實體螢幕真的關閉）
- ✅ 遠端用戶只能看到虛擬顯示器
- ✅ 本地用戶無法看到任何內容
- ✅ 不依賴視窗覆蓋或 API 排除技巧

**缺點：**
- ❌ 實作最複雜
- ❌ 需要額外的虛擬顯示器驅動程式（Amyuni 或 RustDesk IDD）
- ❌ 必須安裝為系統服務才能使用
- ❌ **非同步執行，可能需要 7.5 秒**才能完成
- ❌ 驅動程式初始化緩慢（某些筆電可能需要更長時間）
- ❌ 可能導致顯示器配置混亂
- ❌ 恢復時可能失敗，需要登錄檔恢復機制
- ❌ Windows 24H2 需要特殊處理（Issue #12114）
- ❌ 如果沒有實體顯示器，會返回錯誤（NO_PHYSICAL_DISPLAYS）
- ❌ 驅動程式相容性問題（不是所有系統都支援）
- ❌ 短時間內拔插 Amyuni IDD 可能導致崩潰

**實作評估：**
這是**最後手段**，僅在策略 1 和 2 都不可用時使用。提供最強的隱私保護，但複雜度、延遲和風險都是最高的。

---

### 1.3 輸入控制機制

**檔案位置：** `win_input.rs`

**實作方式：** Windows 低階鉤子 (Low-Level Hooks)

#### 鍵盤攔截：
```rust
SetWindowsHookExA(WH_KEYBOARD_LL, privacy_mode_hook_keyboard, ...)

攔截規則：
1. 允許 Ctrl+P（緊急退出隱私模式）
2. 封鎖所有 Alt 鍵組合（防止 Alt+Tab 切換視窗）
3. 封鎖除了 P、Ctrl (vkCode: 80, 162, 163) 以外的所有按鍵
4. 檢測 enigo::ENIGO_INPUT_EXTRA_VALUE 以區分 RustDesk 產生的輸入
```

#### 滑鼠攔截：
```rust
SetWindowsHookExA(WH_MOUSE_LL, privacy_mode_hook_mouse, ...)

攔截規則：
1. 封鎖所有非 RustDesk 產生的滑鼠輸入
2. 檢測 dwExtraInfo == ENIGO_INPUT_EXTRA_VALUE
3. 只允許遠端輸入通過
```

#### 執行緒管理：
```rust
1. 鉤子在獨立執行緒中執行
2. 使用 Windows 訊息迴圈 (GetMessageA/DispatchMessageA)
3. 透過 PostThreadMessageA(WM_USER_EXIT_HOOK) 優雅關閉
4. 追蹤鉤子執行緒 ID (CUR_HOOK_THREAD_ID)
```

**優點：**
- ✅ 低階鉤子可靠性高
- ✅ 防止本地用戶干擾遠端操作
- ✅ 提供緊急退出機制（Ctrl+P）
- ✅ 區分遠端和本地輸入

**缺點：**
- ❌ 可能被防毒軟體標記為可疑行為
- ❌ Ctrl+Alt+Del 無法攔截（系統保護）
- ❌ 如果鉤子執行緒崩潰，輸入控制失效
- ❌ GetKeyState() 可能不穩定（程式碼中有註記）

---

### 1.4 策略選擇邏輯

**預設策略優先順序（`privacy_mode.rs:89-110`）：**
```rust
1. 首選：win_exclude_from_capture（如果 Windows 10 Build 19041+）
2. 次選：win_mag（如果支援放大鏡模式）
3. 最終：win_virtual_display（如果已安裝為系統服務）
4. 否則：無可用隱私模式（返回空字串）
```

**支援的實作列表（`get_supported_privacy_mode_impl()`）：**
```rust
Windows:
- PRIVACY_MODE_IMPL_WIN_EXCLUDE_FROM_CAPTURE (如果支援)
  或 PRIVACY_MODE_IMPL_WIN_MAG (如果支援)
- PRIVACY_MODE_IMPL_WIN_VIRTUAL_DISPLAY (如果已安裝服務)

macOS/Linux:
- 空列表（不支援）
```

**動態切換：**
- 可以在執行時切換策略（`switch(impl_key)`）
- 如果同一連線請求不同策略，會先關閉舊策略再啟動新策略
- 只有啟動隱私模式的連線才能關閉它（防止干擾）

---

### 1.5 錯誤處理和恢復機制

#### 虛擬顯示器模式的恢復：
```rust
1. TurnOnGuard - RAII 守衛確保失敗時自動恢復
   - 如果 turn_on_privacy 失敗，Drop trait 自動執行 turn_off_privacy

2. 登錄檔連接恢復（CONFIG_KEY_REG_RECOVERY）：
   - 啟動前保存登錄檔連接設定
   - 關閉時恢復登錄檔設定
   - 提供 restore_reg_connectivity() 強制恢復

3. 顯示器設定恢復：
   - 保存完整 DEVMODEW 結構
   - 恢復主顯示器標誌
   - 恢復精確位置和解析度
```

#### 放大鏡模式的驗證：
```rust
check_privacy_mode_err() {
    // 只對放大鏡模式執行
    if is_current_privacy_mode_impl(PRIVACY_MODE_IMPL_WIN_MAG) {
        // 測試建立 capturer 以驗證隱私視窗已正確排除
        test_create_capturer()
    }
}
```

#### 狀態管理：
```rust
PrivacyModeState:
- OffSucceeded  // 正常關閉
- OffByPeer     // 被本地用戶關閉（Ctrl+P）
- OffUnknown    // 未知原因關閉
```

---

## 二、RustDesk 隱私模式優缺點總結

### 2.1 整體優點

#### ✅ 多策略支援
- 提供三種不同策略，適應不同系統版本和硬體
- 自動選擇最佳策略
- 可手動切換策略

#### ✅ 全面的輸入控制
- 鍵盤和滑鼠都有攔截
- 提供緊急退出機制（Ctrl+P）
- 區分遠端和本地輸入

#### ✅ 錯誤恢復機制
- RAII 守衛自動恢復
- 登錄檔備份和恢復
- 顯示器設定完整保存

#### ✅ 程式碼架構清晰
- 使用 Trait 抽象不同實作
- 模組化設計，職責分明
- 策略模式應用得當

### 2.2 整體缺點

#### ❌ 僅支援 Windows
- macOS 和 Linux 完全無實作
- 跨平台支援不足
- 與競爭對手相比是明顯劣勢

#### ❌ 複雜度高
- 三種策略維護成本高
- 虛擬顯示器模式特別複雜
- DLL 注入增加安全風險

#### ❌ 效能和延遲問題
- 虛擬顯示器模式可能需要 7.5 秒
- 驅動程式初始化緩慢
- 非同步執行增加複雜度

#### ❌ 相容性問題
- 虛擬顯示器驅動不是所有系統都支援
- Windows 24H2 需要特殊處理
- 某些硬體配置可能失敗

#### ❌ 安全隱患
- DLL 注入可能被標記為惡意行為
- 鉤子程序可能被防毒軟體攔截
- RuntimeBroker 程序可能引起懷疑

---

## 三、其他遠端操作工具隱私模式比較

### 3.1 TeamViewer Black Screen

**官方文件：** [TeamViewer Black Screen](https://www.teamviewer.com/en-us/global/support/knowledge-base/teamviewer-remote/security/teamviewer-black-screen/)

#### 實作方式：
```
1. 使用專屬的 TeamViewer 監視器驅動程式
2. 驅動程式級別的螢幕遮蔽
3. 視覺安全層替換遠端機器顯示內容
```

#### 平台支援：
- 從任何桌面平台 OS（Windows、macOS、Linux）連線
- 到任何 Windows 7/8/10 裝置、Linux、macOS（TeamViewer 15.8+）

#### 優點：
- ✅ **驅動程式級別實作，更可靠**
- ✅ **跨平台支援完整**（Windows、macOS、Linux）
- ✅ 作為授權客戶標準功能
- ✅ 可設定為預設啟用
- ✅ 結合輸入停用功能

#### 缺點：
- ❌ 需要硬體相容性（不是所有監視器/顯示卡/主機板都支援）
- ❌ 依賴硬體廠商實作
- ❌ 不相容的主機電腦無法使用
- ❌ 仍允許 Ctrl+Alt+Del（Windows/Linux）或 Cmd+Option+Esc（macOS）

#### 配置選項：
```
設定為預設：
1. 勾選「停用本地輸入」（Disable local input for incoming connections）
2. 勾選「啟用黑屏」（Enable Blackscreen for incoming connections）
```

**比較評價：**
- TeamViewer 的實作**更成熟、更穩定**
- **跨平台支援遠優於 RustDesk**
- 硬體相容性問題是主要限制
- 作為商業產品，驅動程式簽章和信任度更高

---

### 3.2 AnyDesk Privacy Mode

**官方文件：** [AnyDesk Privacy Mode](https://anydesk.com/en/features/privacy-mode) | [Screen Privacy](https://support.anydesk.com/docs/screen-privacy)

#### 實作方式：
```
1. 停用遠端顯示器（disable the screen）
2. 將螢幕變黑
3. 封鎖遠端端的輸入和聲音
4. 持續到會話結束或手動停用
```

#### 平台支援：
- Windows 8.1+
- macOS 10.13+
- Linux

#### 優點：
- ✅ **跨平台支援**（Windows、macOS、Linux）
- ✅ 同時封鎖輸入和聲音
- ✅ 權限控制機制（需要雙方授權）
- ✅ 整合在工具列中，操作簡單

#### 缺點：
- ❌ 不支援 Windows 7
- ❌ 需要作業系統特權（某些情況）
- ❌ 需要硬體支援（某些情況）
- ❌ 多顯示卡配置可能失敗
  - **重要限制：** AnyDesk 必須在與主顯示器相同的顯示卡上執行

#### 權限機制：
```
1. 遠端客戶端需授予「啟用隱私模式」權限
2. 連線客戶端可透過工具列「權限」啟用
3. 雙向授權確保安全性
```

**失敗情況：**
- 遠端客戶端不支援隱私模式
- 客戶端版本過舊
- 缺少作業系統所需特權
- 缺少硬體支援
- 多顯示卡配置錯誤

**比較評價：**
- AnyDesk 的實作**簡潔且跨平台**
- **權限機制設計良好**，防止濫用
- 硬體和多顯示卡問題是主要限制
- 比 RustDesk 更容易設定和使用

---

### 3.3 Chrome Remote Desktop Curtain Mode

**官方文件：** [Chrome Remote Desktop Curtain Mode Guide](https://www.airdroid.com/remote-support/chrome-remote-desktop-curtain-mode/) | [Oudel Blog](https://blog.oudel.com/chrome-remote-desktop-curtain-mode-on-windows-10/)

#### 實作方式：
```
1. 整合 Windows 遠端桌面協定（RDP）機制
2. 透過登錄檔設定啟用強制 Curtain
3. 遮蔽遠端電腦螢幕
4. 防止實體在場者看到螢幕內容
```

#### 平台支援：
- **僅 Windows Professional、Ultimate、Enterprise 或 Server**
- 主機和客戶端都需要 Pro（或更高）版本
- ❌ 不支援行動裝置（智慧型手機/平板）

#### 實作方法 1：登錄檔編輯器
```
使用 Regedit 設定以下四個鍵值：

1. HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Terminal Server\fDenyTSConnections = 0
2. HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Terminal Server\WinStations\RDP-Tcp\UserAuthentication = 0
3. HKEY_LOCAL_MACHINE\Software\Policies\Google\Chrome\RemoteAccessHostRequireCurtain = 1
4. (Windows 10) HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Terminal Server\WinStations\RDP-Tcp\SecurityLayer = 1
```

#### 實作方法 2：命令提示字元
```
使用單一命令設定所有必要的登錄檔值並重新啟動 Chrome Remote Desktop 服務
```

#### 優點：
- ✅ **利用 Windows 內建 RDP 機制**
- ✅ 不需要額外的驅動程式
- ✅ 系統級別的遮蔽，可靠性高
- ✅ 設定後自動套用於所有會話

#### 缺點：
- ❌ **僅支援 Windows（且只有 Pro 以上版本）**
- ❌ 不支援 macOS 或 Linux
- ❌ 不支援行動裝置
- ❌ 需要手動編輯登錄檔或執行命令
- ❌ 需要重新啟動服務
- ❌ Windows Home 版本完全無法使用

**比較評價：**
- Chrome Remote Desktop 的實作**最簡單**
- **完全依賴 Windows RDP 基礎設施**
- 平台限制非常嚴格（僅 Windows Pro+）
- 對於企業環境很理想，但個人用戶可能無法使用

---

### 3.4 Splashtop（補充資料）

**文件：** [Splashtop Black Screen Support](https://support-splashtopbusiness.splashtop.com/hc/en-us/articles/212724063)

#### 實作方式：
```
1. 主要用於無頭（headless）Windows PC
2. 使用虛擬顯示器驅動程式
3. 解決遠端連線時的顯示問題或黑屏問題
```

#### 特點：
- ✅ 專注於無頭 PC 場景
- ✅ 虛擬顯示器驅動整合
- ❌ 文件主要描述問題排除，而非隱私功能

**比較評價：**
- Splashtop 的方法與 RustDesk 的虛擬顯示器策略類似
- 但主要用於無頭系統，不是專門的隱私功能

---

## 四、綜合比較表

| 特性 | RustDesk | TeamViewer | AnyDesk | Chrome Remote Desktop |
|------|----------|------------|---------|----------------------|
| **Windows 支援** | ✅ 三種策略 | ✅ 驅動程式 | ✅ 是 | ✅ Pro 版本限定 |
| **macOS 支援** | ❌ 無 | ✅ 是 | ✅ 10.13+ | ❌ 無 |
| **Linux 支援** | ❌ 無 | ✅ 是 | ✅ 是 | ❌ 無 |
| **實作複雜度** | ⚠️ 高（3 種策略）| 🟢 中等 | 🟢 中等 | 🟢 低（利用 RDP）|
| **啟動速度** | ⚠️ 0.1-7.5 秒 | 🟢 快 | 🟢 快 | ⚠️ 需重啟服務 |
| **驅動程式需求** | ⚠️ 策略 3 需要 | ⚠️ 需要 | ⚠️ 可能需要 | ❌ 不需要 |
| **硬體相容性** | ⚠️ 策略 3 有問題 | ⚠️ 不是所有硬體 | ⚠️ 多顯示卡問題 | 🟢 好 |
| **隱私強度** | 🟢 策略 3 最強 | 🟢 強 | 🟢 強 | 🟢 強 |
| **輸入控制** | ✅ 是（鉤子）| ✅ 是 | ✅ 是 | ⚠️ 部分 |
| **緊急退出** | ✅ Ctrl+P | ⚠️ Ctrl+Alt+Del | ⚠️ 部分 | ⚠️ 部分 |
| **權限機制** | ⚠️ 基本 | ✅ 完整 | ✅ 雙向授權 | ⚠️ 登錄檔 |
| **易用性** | ⚠️ 自動選策略 | ✅ 簡單 | ✅ 工具列整合 | ❌ 需手動設定 |
| **商業成熟度** | ⚠️ 開源專案 | ✅ 成熟商業 | ✅ 成熟商業 | ✅ Google 產品 |

### 評分圖例：
- ✅ 完整支援
- 🟢 良好
- ⚠️ 有限制/問題
- ❌ 不支援

---

## 五、改進建議

基於以上分析，以下是 RustDesk 隱私模式的改進建議：

### 5.1 高優先級改進

#### 🔴 1. 增加 macOS 和 Linux 支援
**理由：** 這是與競爭對手相比最大的差距

**建議實作方向：**

**macOS:**
```
選項 A：使用 CGDisplayCapture API
- CGDisplayCapture() 可以獨占顯示器
- 產生黑屏效果
- 系統 API，不需要額外驅動

選項 B：使用 ScreenCaptureKit（macOS 12.3+）
- 更現代的螢幕擷取 API
- 可以排除特定視窗

選項 C：建立全螢幕黑色視窗
- 類似 Windows 放大鏡策略
- 使用 NSWindow 的 setLevel(NSWindow.Level.screenSaver)
- 結合輸入攔截（CGEventTap）
```

**Linux:**
```
選項 A：X11 環境
- 建立全螢幕 X11 視窗覆蓋
- 使用 XGrabKeyboard/XGrabPointer 攔截輸入
- 攔截 XRecordExtension 記錄事件

選項 B：Wayland 環境
- 更複雜，Wayland 安全模型限制較多
- 可能需要與合成器（compositor）協作
- 考慮使用虛擬輸出（virtual output）

選項 C：混合方法
- 偵測目前環境（X11/Wayland）
- 提供不同實作
```

#### 🔴 2. 簡化實作策略
**理由：** 三種策略維護成本高，複雜度大

**建議：**
```
1. 保留策略 1（Exclude from Capture）作為快速選項
2. 合併策略 2 和 3：
   - 移除 DLL 注入方案（安全風險高）
   - 統一使用虛擬顯示器方案
   - 改進虛擬顯示器驅動的可靠性和速度
3. 或者：完全採用策略 1 + 輸入攔截
   - 如果隱私需求不是「完全隱藏本地螢幕」
   - 大幅簡化實作
```

#### 🔴 3. 改進虛擬顯示器效能
**理由：** 7.5 秒延遲不可接受

**建議：**
```
1. 驅動程式優化：
   - 優化 IDD 驅動初始化流程
   - 使用更快的驅動載入機制
   - 預先建立虛擬顯示器池

2. 使用者體驗改進：
   - 顯示進度條（目前 X%）
   - 提供「快速模式」（策略 1）和「安全模式」（策略 3）選項
   - 讓使用者選擇可接受的啟動時間

3. 快取機制：
   - 首次連線後保持虛擬顯示器活動
   - 後續連線立即切換（<1 秒）
```

### 5.2 中優先級改進

#### 🟡 4. 增強權限和授權機制
**理由：** 防止濫用，符合企業安全需求

**建議：**
```
1. 雙向授權機制（參考 AnyDesk）：
   - 本地用戶必須明確授予「隱私模式」權限
   - 遠端用戶需要明確請求

2. 政策設定：
   - 管理員可以強制啟用/停用隱私模式
   - 可以限定特定使用者或群組

3. 審計日誌：
   - 記錄隱私模式的啟動/關閉時間
   - 記錄是誰啟動的、持續時間
```

#### 🟡 5. 改進錯誤恢復
**理由：** 避免使用者無法恢復螢幕

**建議：**
```
1. 多重恢復機制：
   - 主恢復：正常 turn_off_privacy()
   - 備援 1：TurnOnGuard Drop
   - 備援 2：登錄檔恢復
   - 備援 3：系統重新啟動後自動恢復

2. 安全模式：
   - 如果偵測到多次失敗，自動停用隱私模式
   - 提供「恢復顯示器」工具

3. 使用者通知：
   - 如果隱私模式啟動失敗，清楚告知原因
   - 提供疑難排解步驟
```

#### 🟡 6. 移除 DLL 注入（策略 2）
**理由：** 安全風險高，容易被標記為惡意軟體

**建議：**
```
1. 如果必須保留策略 2：
   - 使用程式碼簽章憑證簽署所有元件
   - 提供白名單指引給防毒軟體
   - 在安裝時明確告知使用者

2. 替代方案：
   - 使用獨立的輔助程式（非注入）
   - 使用 Windows 合法的 COM 註冊機制
   - 考慮策略 1 的增強版本
```

### 5.3 低優先級改進

#### 🟢 7. 使用者體驗優化
```
1. 提供隱私模式預覽：
   - 連線前顯示哪種策略會被使用
   - 顯示預期的啟動時間

2. 自訂選項：
   - 允許使用者選擇緊急退出快捷鍵（不只 Ctrl+P）
   - 允許自訂黑屏顏色或顯示警告訊息

3. 行動端支援：
   - 考慮在行動裝置客戶端也提供隱私模式控制
```

#### 🟢 8. 測試和驗證
```
1. 自動化測試：
   - 模擬不同硬體配置
   - 測試所有三種策略的切換
   - 測試失敗恢復流程

2. 硬體相容性資料庫：
   - 收集使用者回報的相容性資訊
   - 提供相容性檢查工具
   - 在連線前警告潛在問題
```

#### 🟢 9. 文件和透明度
```
1. 技術文件：
   - 詳細說明每種策略的運作原理
   - 提供安全性和隱私評估
   - 說明權限需求和風險

2. 使用者指南：
   - 針對不同場景推薦策略
   - 疑難排解指南
   - 常見問題解答
```

---

## 六、總結

### 6.1 RustDesk 的優勢
1. **多策略彈性**：提供三種策略適應不同環境
2. **開源透明**：程式碼公開，可審查
3. **完整輸入控制**：鍵盤和滑鼠都有攔截
4. **錯誤恢復機制**：RAII 和登錄檔備份

### 6.2 RustDesk 的劣勢
1. **僅支援 Windows**：與競爭對手相比最大弱點
2. **實作過於複雜**：三種策略維護成本高
3. **效能問題**：虛擬顯示器模式延遲高
4. **安全隱患**：DLL 注入可能被標記

### 6.3 核心建議

#### 立即行動（高優先級）：
1. **增加 macOS 和 Linux 支援** - 這是最迫切的需求
2. **簡化實作策略** - 減少維護負擔
3. **改進虛擬顯示器效能** - 7.5 秒不可接受

#### 中期規劃（中優先級）：
4. **增強權限機制** - 符合企業需求
5. **改進錯誤恢復** - 避免使用者困擾
6. **移除 DLL 注入** - 降低安全風險

#### 長期優化（低優先級）：
7. **使用者體驗優化**
8. **測試和驗證**
9. **文件和透明度**

### 6.4 最終評價

RustDesk 的隱私模式實作在 Windows 平台上是**功能完整且有深度**的，提供了業界少見的多策略選擇。然而，**缺乏 macOS 和 Linux 支援是致命傷**，使其無法與 TeamViewer 和 AnyDesk 競爭。

建議優先投入資源開發跨平台支援，同時簡化現有實作以降低維護成本。如果能夠實現這兩個目標，RustDesk 的隱私模式將成為一大競爭優勢。

---

## 參考資料

### RustDesk 原始碼
- `/home/user/rustdesk/src/privacy_mode.rs` - 核心模組
- `/home/user/rustdesk/src/privacy_mode/win_virtual_display.rs` - 虛擬顯示器實作
- `/home/user/rustdesk/src/privacy_mode/win_mag.rs` - 放大鏡實作
- `/home/user/rustdesk/src/privacy_mode/win_input.rs` - 輸入攔截

### 外部文件
- [TeamViewer Black Screen](https://www.teamviewer.com/en-us/global/support/knowledge-base/teamviewer-remote/security/teamviewer-black-screen/)
- [TeamViewer Policy Settings](https://www.teamviewer.com/en-us/global/support/knowledge-base/teamviewer-remote/devices/policy-settings/)
- [How to Black Screen in TeamViewer](https://tsplus.net/remote-support/blog/how-to-black-screen-in-teamviewer/)
- [AnyDesk Privacy Mode](https://anydesk.com/en/features/privacy-mode)
- [AnyDesk Screen Privacy Documentation](https://support.anydesk.com/docs/screen-privacy)
- [AnyDesk Privacy Mode Troubleshooting](https://www.oreateai.com/blog/troubleshooting-anydesks-privacy-mode-what-to-do-when-it-fails/f7f6b8c296a693656b54dda118dc9a67)
- [Chrome Remote Desktop Curtain Mode Guide](https://www.airdroid.com/remote-support/chrome-remote-desktop-curtain-mode/)
- [Chrome Remote Desktop Curtain Mode - AnyViewer](https://www.anyviewer.com/how-to/chrome-remote-desktop-curtain-mode-2578.html)
- [Chrome Remote Desktop on Windows 10 - Oudel Blog](https://blog.oudel.com/chrome-remote-desktop-curtain-mode-on-windows-10/)
- [Virtual Display Driver GitHub](https://github.com/VirtualDrivers/Virtual-Display-Driver)
- [Parsec VDD GitHub](https://github.com/nomi-san/parsec-vdd)

---

**文件版本：** 1.0
**分析日期：** 2026-01-19
**分析者：** Claude (Anthropic)
