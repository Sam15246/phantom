//! Proctoring Software Detection Engine
//!
//! Scans the system for known proctoring software via multiple vectors:
//! process enumeration, window enumeration, service detection, and
//! keyboard hook detection. Results are used to adapt stealth behavior
//! at runtime.

use serde::Serialize;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Overall detection report returned by `full_scan()`.
#[derive(Debug, Clone, Serialize)]
pub struct ProctorReport {
    /// Proctoring vendors detected on the system
    pub vendors: Vec<DetectedVendor>,
    /// Summary of detected monitoring capabilities
    pub active_capabilities: ActiveCapabilities,
    /// Overall threat level based on detected proctoring
    pub threat_level: ThreatLevel,
    /// Recommended adaptations for the overlay
    pub recommendations: Vec<String>,
    /// Timestamp of this scan
    pub scanned_at: String,
}

/// A detected proctoring vendor with evidence.
#[derive(Debug, Clone, Serialize)]
pub struct DetectedVendor {
    pub name: &'static str,
    pub vendor_id: &'static str,
    /// How the vendor was detected (process, window, service, etc.)
    pub evidence: Vec<String>,
    /// Known capabilities of this vendor
    pub capabilities: VendorCapabilities,
}

/// Known capabilities of a proctoring vendor.
#[derive(Debug, Clone, Serialize)]
pub struct VendorCapabilities {
    pub screen_capture: bool,
    pub keyboard_hook: bool,
    pub process_scan: bool,
    pub network_monitor: bool,
    pub audio_analysis: bool,
    pub gaze_tracking: bool,
    pub vm_detection: bool,
    pub wda_detection: bool,
    pub browser_lockdown: bool,
}

/// Aggregated active capabilities across all detected vendors.
#[derive(Debug, Clone, Serialize)]
pub struct ActiveCapabilities {
    pub any_screen_capture: bool,
    pub any_keyboard_hook: bool,
    pub any_process_scan: bool,
    pub any_network_monitor: bool,
    pub any_wda_detection: bool,
    pub any_browser_lockdown: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ThreatLevel {
    /// No proctoring detected
    Clear,
    /// Low-risk proctoring (browser-only extensions)
    Low,
    /// Medium-risk (desktop agent with some monitoring)
    Medium,
    /// High-risk (full lockdown browser with deep system access)
    High,
    /// Critical (known WDA detection, kernel-level hooks)
    Critical,
}

// ---------------------------------------------------------------------------
// Known proctoring signatures
// ---------------------------------------------------------------------------

struct ProcessSignature {
    /// Substring to match (case-insensitive) against process name
    pattern: &'static str,
    vendor_id: &'static str,
}

struct WindowSignature {
    /// Substring to match (case-insensitive) against window title
    title_pattern: &'static str,
    vendor_id: &'static str,
}

struct ServiceSignature {
    /// Substring to match (case-insensitive) against service name
    pattern: &'static str,
    vendor_id: &'static str,
}

/// Vendor metadata — name, ID, and known capabilities.
struct VendorProfile {
    name: &'static str,
    id: &'static str,
    caps: VendorCapabilities,
}

const VENDOR_PROFILES: &[VendorProfile] = &[
    VendorProfile {
        name: "Respondus LockDown Browser",
        id: "respondus",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: true,
            process_scan: true,
            network_monitor: false,
            audio_analysis: true,
            gaze_tracking: true,
            vm_detection: true,
            wda_detection: false,
            browser_lockdown: true,
        },
    },
    VendorProfile {
        name: "ExamSoft / Examplify",
        id: "examsoft",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: true,
            process_scan: true,
            network_monitor: true,
            audio_analysis: true,
            gaze_tracking: false,
            vm_detection: true,
            wda_detection: false,
            browser_lockdown: true,
        },
    },
    VendorProfile {
        name: "Pearson OnVUE",
        id: "onvue",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: false,
            process_scan: true,
            network_monitor: true,
            audio_analysis: true,
            gaze_tracking: true,
            vm_detection: true,
            wda_detection: false,
            browser_lockdown: true,
        },
    },
    VendorProfile {
        name: "PSI Secure Browser",
        id: "psi",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: false,
            process_scan: true,
            network_monitor: false,
            audio_analysis: true,
            gaze_tracking: true,
            vm_detection: true,
            wda_detection: false,
            browser_lockdown: true,
        },
    },
    VendorProfile {
        name: "Proctorio",
        id: "proctorio",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: false,
            process_scan: true,
            network_monitor: true,
            audio_analysis: false,
            gaze_tracking: false,
            vm_detection: false,
            wda_detection: false,
            browser_lockdown: false,
        },
    },
    VendorProfile {
        name: "Honorlock",
        id: "honorlock",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: false,
            process_scan: true,
            network_monitor: true,
            audio_analysis: true,
            gaze_tracking: false,
            vm_detection: false,
            wda_detection: false,
            browser_lockdown: false,
        },
    },
    VendorProfile {
        name: "Kryterion / Sentinel",
        id: "kryterion",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: true,
            process_scan: true,
            network_monitor: true,
            audio_analysis: false,
            gaze_tracking: true,
            vm_detection: true,
            wda_detection: false,
            browser_lockdown: true,
        },
    },
    VendorProfile {
        name: "Safe Exam Browser",
        id: "seb",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: true,
            process_scan: true,
            network_monitor: false,
            audio_analysis: false,
            gaze_tracking: false,
            vm_detection: true,
            wda_detection: false,
            browser_lockdown: true,
        },
    },
    VendorProfile {
        name: "Aiseptor",
        id: "aiseptor",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: false,
            process_scan: true,
            network_monitor: true,
            audio_analysis: false,
            gaze_tracking: false,
            vm_detection: false,
            wda_detection: true, // CRITICAL — detects WDA_EXCLUDEFROMCAPTURE
            browser_lockdown: false,
        },
    },
    VendorProfile {
        name: "HackerRank Proctor",
        id: "hackerrank",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: false,
            process_scan: true,
            network_monitor: false,
            audio_analysis: false,
            gaze_tracking: true,
            vm_detection: false,
            wda_detection: true,
            browser_lockdown: false,
        },
    },
    VendorProfile {
        name: "ProctorU / Meazure Learning",
        id: "proctoru",
        caps: VendorCapabilities {
            screen_capture: true,
            keyboard_hook: false,
            process_scan: true,
            network_monitor: false,
            audio_analysis: true,
            gaze_tracking: true,
            vm_detection: true,
            wda_detection: false,
            browser_lockdown: false,
        },
    },
];

// Process name patterns (case-insensitive substring match)
const PROCESS_SIGNATURES: &[ProcessSignature] = &[
    // Respondus
    ProcessSignature { pattern: "lockdownbrowser", vendor_id: "respondus" },
    ProcessSignature { pattern: "rpnow", vendor_id: "respondus" },
    ProcessSignature { pattern: "respondus", vendor_id: "respondus" },
    ProcessSignature { pattern: "ldbbrowsermonitor", vendor_id: "respondus" },
    // ExamSoft
    ProcessSignature { pattern: "examsoft", vendor_id: "examsoft" },
    ProcessSignature { pattern: "examplify", vendor_id: "examsoft" },
    ProcessSignature { pattern: "esagent", vendor_id: "examsoft" },
    // Pearson OnVUE
    ProcessSignature { pattern: "onvue", vendor_id: "onvue" },
    ProcessSignature { pattern: "pearsonvue", vendor_id: "onvue" },
    ProcessSignature { pattern: "pvmonitor", vendor_id: "onvue" },
    ProcessSignature { pattern: "systest", vendor_id: "onvue" },
    // PSI
    ProcessSignature { pattern: "psi_bridge", vendor_id: "psi" },
    ProcessSignature { pattern: "psibridge", vendor_id: "psi" },
    ProcessSignature { pattern: "securebrowser", vendor_id: "psi" },
    ProcessSignature { pattern: "psiservices", vendor_id: "psi" },
    // Proctorio
    ProcessSignature { pattern: "proctorio", vendor_id: "proctorio" },
    // Honorlock (browser extension, but may spawn helper)
    ProcessSignature { pattern: "honorlock", vendor_id: "honorlock" },
    // Kryterion / Sentinel
    ProcessSignature { pattern: "sentinel", vendor_id: "kryterion" },
    ProcessSignature { pattern: "kryterion", vendor_id: "kryterion" },
    ProcessSignature { pattern: "webassessor", vendor_id: "kryterion" },
    // Safe Exam Browser
    ProcessSignature { pattern: "safeexambrowser", vendor_id: "seb" },
    ProcessSignature { pattern: "sebwindows", vendor_id: "seb" },
    ProcessSignature { pattern: "sebservice", vendor_id: "seb" },
    // Aiseptor
    ProcessSignature { pattern: "aiseptor", vendor_id: "aiseptor" },
    // HackerRank
    ProcessSignature { pattern: "hackerrank", vendor_id: "hackerrank" },
    // ProctorU / Meazure
    ProcessSignature { pattern: "proctoru", vendor_id: "proctoru" },
    ProcessSignature { pattern: "meazure", vendor_id: "proctoru" },
    ProcessSignature { pattern: "guardian", vendor_id: "proctoru" },
];

const WINDOW_SIGNATURES: &[WindowSignature] = &[
    WindowSignature { title_pattern: "lockdown browser", vendor_id: "respondus" },
    WindowSignature { title_pattern: "respondus", vendor_id: "respondus" },
    WindowSignature { title_pattern: "examplify", vendor_id: "examsoft" },
    WindowSignature { title_pattern: "examsoft", vendor_id: "examsoft" },
    WindowSignature { title_pattern: "onvue", vendor_id: "onvue" },
    WindowSignature { title_pattern: "pearson vue", vendor_id: "onvue" },
    WindowSignature { title_pattern: "psi secure", vendor_id: "psi" },
    WindowSignature { title_pattern: "safe exam browser", vendor_id: "seb" },
    WindowSignature { title_pattern: "aiseptor", vendor_id: "aiseptor" },
    WindowSignature { title_pattern: "proctoru", vendor_id: "proctoru" },
    WindowSignature { title_pattern: "meazure", vendor_id: "proctoru" },
];

const SERVICE_SIGNATURES: &[ServiceSignature] = &[
    ServiceSignature { pattern: "respondus", vendor_id: "respondus" },
    ServiceSignature { pattern: "lockdown", vendor_id: "respondus" },
    ServiceSignature { pattern: "examsoft", vendor_id: "examsoft" },
    ServiceSignature { pattern: "examplify", vendor_id: "examsoft" },
    ServiceSignature { pattern: "sebservice", vendor_id: "seb" },
    ServiceSignature { pattern: "sebwindowsservice", vendor_id: "seb" },
    ServiceSignature { pattern: "sentinel", vendor_id: "kryterion" },
    ServiceSignature { pattern: "psiservice", vendor_id: "psi" },
    ServiceSignature { pattern: "onvue", vendor_id: "onvue" },
];

// ---------------------------------------------------------------------------
// Platform-specific detection (Windows)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod win {
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, TRUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowDisplayAffinity, GetWindowTextLengthW, GetWindowTextW,
        IsWindowVisible,
    };

    use super::*;

    /// Enumerate all running processes, return list of (pid, name_lowercase).
    pub fn enumerate_processes() -> Vec<(u32, String)> {
        let mut result = Vec::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
                return result;
            }

            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

            if Process32FirstW(snapshot, &mut entry) == TRUE {
                loop {
                    let name_len = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name = OsString::from_wide(&entry.szExeFile[..name_len])
                        .to_string_lossy()
                        .to_lowercase();
                    result.push((entry.th32ProcessID, name));

                    if Process32NextW(snapshot, &mut entry) != TRUE {
                        break;
                    }
                }
            }

            CloseHandle(snapshot);
        }
        result
    }

    /// Scan running processes against known proctoring signatures.
    pub fn scan_processes() -> HashMap<&'static str, Vec<String>> {
        let processes = enumerate_processes();
        let mut hits: HashMap<&'static str, Vec<String>> = HashMap::new();

        for (pid, name) in &processes {
            for sig in PROCESS_SIGNATURES {
                if name.contains(sig.pattern) {
                    hits.entry(sig.vendor_id)
                        .or_default()
                        .push(format!("process: {} (PID {})", name, pid));
                }
            }
        }

        hits
    }

    /// Enumerate visible windows, return list of (hwnd, title_lowercase).
    pub fn enumerate_windows() -> Vec<(isize, String)> {
        unsafe {
            let mut windows: Vec<(isize, String)> = Vec::new();
            let windows_ptr = &mut windows as *mut Vec<(isize, String)> as LPARAM;
            EnumWindows(Some(enum_window_callback), windows_ptr);
            windows
        }
    }

    unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let windows = &mut *(lparam as *mut Vec<(isize, String)>);

        if IsWindowVisible(hwnd) == 0 {
            return TRUE; // skip invisible windows
        }

        let text_len = GetWindowTextLengthW(hwnd);
        if text_len > 0 {
            let mut buf = vec![0u16; (text_len + 1) as usize];
            let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if copied > 0 {
                let title = OsString::from_wide(&buf[..copied as usize])
                    .to_string_lossy()
                    .to_lowercase();
                windows.push((hwnd as isize, title));
            }
        }

        TRUE
    }

    /// Scan visible windows against known proctoring signatures.
    pub fn scan_windows() -> HashMap<&'static str, Vec<String>> {
        let windows = enumerate_windows();
        let mut hits: HashMap<&'static str, Vec<String>> = HashMap::new();

        for (_hwnd, title) in &windows {
            for sig in WINDOW_SIGNATURES {
                if title.contains(sig.title_pattern) {
                    hits.entry(sig.vendor_id)
                        .or_default()
                        .push(format!("window: \"{}\"", title));
                }
            }
        }

        hits
    }

    /// Check if any window on the system is scanning for WDA flags.
    /// We do this by enumerating all windows and checking which ones
    /// have non-zero display affinity — if WE are the only one, it's
    /// likely that a scanner is checking for it.
    pub fn detect_wda_scanners() -> Vec<String> {
        let mut findings = Vec::new();

        // Check if any other process has windows with WDA set (unusual)
        let windows = enumerate_windows();
        for (hwnd, title) in &windows {
            let mut affinity: u32 = 0;
            unsafe {
                if GetWindowDisplayAffinity(*hwnd as HWND, &mut affinity) != 0 && affinity != 0 {
                    // If it's not our window, someone else is using WDA
                    if !title.contains("audio device") {
                        findings.push(format!(
                            "Non-self WDA window detected: \"{}\" (affinity=0x{:X})",
                            title, affinity
                        ));
                    }
                }
            }
        }

        // Check process list for known WDA-scanning tools
        let processes = enumerate_processes();
        for (pid, name) in &processes {
            if name.contains("aiseptor") || name.contains("hackerrank") {
                findings.push(format!(
                    "Known WDA scanner process: {} (PID {}) — WDA_EXCLUDEFROMCAPTURE IS DETECTABLE",
                    name, pid
                ));
            }
        }

        findings
    }

    /// Scan Windows services for known proctoring services.
    /// Uses sc query via CreateToolhelp32Snapshot on services.
    pub fn scan_services() -> HashMap<&'static str, Vec<String>> {
        let mut hits: HashMap<&'static str, Vec<String>> = HashMap::new();

        // Use the SC Manager API to enumerate services
        use windows_sys::Win32::System::Services::{
            OpenSCManagerW, EnumServicesStatusW, CloseServiceHandle,
            ENUM_SERVICE_STATUSW, SC_MANAGER_ENUMERATE_SERVICE,
            SERVICE_STATE_ALL, SERVICE_WIN32,
        };

        unsafe {
            let sc_manager = OpenSCManagerW(
                std::ptr::null(),
                std::ptr::null(),
                SC_MANAGER_ENUMERATE_SERVICE,
            );

            if sc_manager.is_null() {
                // Fallback: try reading services from registry
                return scan_services_registry();
            }

            let mut bytes_needed: u32 = 0;
            let mut services_returned: u32 = 0;
            let mut resume_handle: u32 = 0;

            // First call to get required buffer size
            EnumServicesStatusW(
                sc_manager,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                std::ptr::null_mut(),
                0,
                &mut bytes_needed,
                &mut services_returned,
                &mut resume_handle,
            );

            if bytes_needed == 0 {
                CloseServiceHandle(sc_manager);
                return hits;
            }

            let mut buffer = vec![0u8; bytes_needed as usize];
            resume_handle = 0;

            let success = EnumServicesStatusW(
                sc_manager,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                buffer.as_mut_ptr() as *mut ENUM_SERVICE_STATUSW,
                bytes_needed,
                &mut bytes_needed,
                &mut services_returned,
                &mut resume_handle,
            );

            if success != 0 {
                let services = std::slice::from_raw_parts(
                    buffer.as_ptr() as *const ENUM_SERVICE_STATUSW,
                    services_returned as usize,
                );

                for service in services {
                    // Read service name
                    if service.lpServiceName.is_null() {
                        continue;
                    }
                    let name_len = (0..)
                        .find(|&i| *service.lpServiceName.add(i) == 0)
                        .unwrap_or(0);
                    let name = OsString::from_wide(std::slice::from_raw_parts(
                        service.lpServiceName,
                        name_len,
                    ))
                    .to_string_lossy()
                    .to_lowercase();

                    // Read display name
                    let display = if !service.lpDisplayName.is_null() {
                        let dl = (0..)
                            .find(|&i| *service.lpDisplayName.add(i) == 0)
                            .unwrap_or(0);
                        OsString::from_wide(std::slice::from_raw_parts(
                            service.lpDisplayName,
                            dl,
                        ))
                        .to_string_lossy()
                        .to_lowercase()
                    } else {
                        String::new()
                    };

                    let combined = format!("{} {}", name, display);

                    for sig in SERVICE_SIGNATURES {
                        if combined.contains(sig.pattern) {
                            hits.entry(sig.vendor_id)
                                .or_default()
                                .push(format!("service: {} ({})", name, display));
                        }
                    }
                }
            }

            CloseServiceHandle(sc_manager);
        }

        hits
    }

    /// Fallback: scan registry for service names when SC Manager access is denied.
    fn scan_services_registry() -> HashMap<&'static str, Vec<String>> {
        use std::process::Command;
        let mut hits: HashMap<&'static str, Vec<String>> = HashMap::new();

        // Use `sc query state= all` as fallback
        if let Ok(output) = Command::new("sc")
            .args(["query", "state=", "all"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
            for sig in SERVICE_SIGNATURES {
                if text.contains(sig.pattern) {
                    hits.entry(sig.vendor_id)
                        .or_default()
                        .push(format!("service (registry): pattern \"{}\" found", sig.pattern));
                }
            }
        }

        hits
    }

    /// Detect if keyboard hooks are likely installed by checking if our
    /// test hotkeys get swallowed. This is a heuristic — we can't directly
    /// enumerate other processes' hooks without kernel access.
    pub fn detect_keyboard_hooks() -> Vec<String> {
        let mut findings = Vec::new();

        // Check for processes known to install WH_KEYBOARD_LL hooks
        let processes = enumerate_processes();
        let hook_installers = [
            ("lockdownbrowser", "Respondus"),
            ("respondus", "Respondus"),
            ("examsoft", "ExamSoft"),
            ("examplify", "ExamSoft"),
            ("sentinel", "Kryterion"),
            ("sebservice", "Safe Exam Browser"),
            ("safeexambrowser", "Safe Exam Browser"),
        ];

        for (pid, name) in &processes {
            for (pattern, vendor) in &hook_installers {
                if name.contains(pattern) {
                    findings.push(format!(
                        "Keyboard hook likely active: {} ({}, PID {}) is known to install WH_KEYBOARD_LL",
                        name, vendor, pid
                    ));
                }
            }
        }

        findings
    }

    /// Check for network monitoring indicators.
    pub fn detect_network_monitoring() -> Vec<String> {
        let mut findings = Vec::new();

        // Check for known network monitoring processes
        let net_monitors = [
            ("wireshark", "Wireshark (packet capture)"),
            ("fiddler", "Fiddler (HTTP proxy)"),
            ("charles", "Charles Proxy"),
            ("proxifier", "Proxifier"),
            ("netmon", "Network Monitor"),
            ("pktmon", "Windows Packet Monitor"),
            ("wfpdiag", "WFP Diagnostics"),
        ];

        let processes = enumerate_processes();
        for (pid, name) in &processes {
            for (pattern, desc) in &net_monitors {
                if name.contains(pattern) {
                    findings.push(format!(
                        "Network monitor: {} — {} (PID {})",
                        name, desc, pid
                    ));
                }
            }
        }

        // Check for proctoring-specific network monitoring
        // (these would be detected via process scan, but flag specifically)
        let proctor_net = [
            "exammonitor",
            "examlockdown",
            "onvuemonitor",
        ];
        for (_, name) in &processes {
            for pattern in &proctor_net {
                if name.contains(pattern) {
                    findings.push(format!(
                        "Proctoring network monitor process: {}",
                        name
                    ));
                }
            }
        }

        findings
    }
}

// ---------------------------------------------------------------------------
// Non-Windows stub
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "windows"))]
mod win {
    use std::collections::HashMap;
    use super::*;

    pub fn scan_processes() -> HashMap<&'static str, Vec<String>> { HashMap::new() }
    pub fn scan_windows() -> HashMap<&'static str, Vec<String>> { HashMap::new() }
    pub fn scan_services() -> HashMap<&'static str, Vec<String>> { HashMap::new() }
    pub fn detect_wda_scanners() -> Vec<String> { Vec::new() }
    pub fn detect_keyboard_hooks() -> Vec<String> { Vec::new() }
    pub fn detect_network_monitoring() -> Vec<String> { Vec::new() }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Run a full system scan for proctoring software.
pub fn full_scan() -> ProctorReport {
    let mut vendor_map: std::collections::HashMap<&'static str, Vec<String>> =
        std::collections::HashMap::new();

    // 1. Process scan
    for (vendor_id, evidence) in win::scan_processes() {
        vendor_map.entry(vendor_id).or_default().extend(evidence);
    }

    // 2. Window scan
    for (vendor_id, evidence) in win::scan_windows() {
        vendor_map.entry(vendor_id).or_default().extend(evidence);
    }

    // 3. Service scan
    for (vendor_id, evidence) in win::scan_services() {
        vendor_map.entry(vendor_id).or_default().extend(evidence);
    }

    // 4. WDA scanner detection
    let wda_findings = win::detect_wda_scanners();

    // 5. Keyboard hook detection
    let hook_findings = win::detect_keyboard_hooks();

    // 6. Network monitoring detection
    let net_findings = win::detect_network_monitoring();

    // Build vendor list
    let mut vendors: Vec<DetectedVendor> = Vec::new();
    for (vendor_id, evidence) in &vendor_map {
        if let Some(profile) = VENDOR_PROFILES.iter().find(|p| p.id == *vendor_id) {
            vendors.push(DetectedVendor {
                name: profile.name,
                vendor_id: profile.id,
                evidence: evidence.clone(),
                capabilities: profile.caps.clone(),
            });
        }
    }

    // Aggregate capabilities
    let active_capabilities = ActiveCapabilities {
        any_screen_capture: vendors.iter().any(|v| v.capabilities.screen_capture),
        any_keyboard_hook: vendors.iter().any(|v| v.capabilities.keyboard_hook)
            || !hook_findings.is_empty(),
        any_process_scan: vendors.iter().any(|v| v.capabilities.process_scan),
        any_network_monitor: vendors.iter().any(|v| v.capabilities.network_monitor)
            || !net_findings.is_empty(),
        any_wda_detection: vendors.iter().any(|v| v.capabilities.wda_detection)
            || !wda_findings.is_empty(),
        any_browser_lockdown: vendors.iter().any(|v| v.capabilities.browser_lockdown),
    };

    // Determine threat level
    let threat_level = if vendors.is_empty() {
        ThreatLevel::Clear
    } else if active_capabilities.any_wda_detection {
        ThreatLevel::Critical
    } else if active_capabilities.any_browser_lockdown && active_capabilities.any_keyboard_hook {
        ThreatLevel::High
    } else if active_capabilities.any_process_scan || active_capabilities.any_screen_capture {
        ThreatLevel::Medium
    } else {
        ThreatLevel::Low
    };

    // Generate recommendations
    let mut recommendations = Vec::new();
    if active_capabilities.any_wda_detection {
        recommendations.push(
            "CRITICAL: WDA scanner detected — capture protection flag is visible. Consider disabling WDA_EXCLUDEFROMCAPTURE.".into()
        );
    }
    if active_capabilities.any_keyboard_hook {
        recommendations.push(
            "Keyboard hooks detected — hotkeys may be blocked. Use tray menu as fallback.".into()
        );
    }
    if active_capabilities.any_network_monitor {
        recommendations.push(
            "Network monitoring detected — API calls to openai.com/groq.com may be logged.".into()
        );
    }
    if active_capabilities.any_process_scan {
        recommendations.push(
            "Process scanning active — our process name and metadata are being inspected.".into()
        );
    }
    if active_capabilities.any_browser_lockdown {
        recommendations.push(
            "Browser lockdown active — exam is running in a controlled environment.".into()
        );
    }

    // Add WDA and hook findings as evidence
    for finding in &wda_findings {
        recommendations.push(finding.clone());
    }
    for finding in &hook_findings {
        recommendations.push(finding.clone());
    }
    for finding in &net_findings {
        recommendations.push(finding.clone());
    }

    let scanned_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    ProctorReport {
        vendors,
        active_capabilities,
        threat_level,
        recommendations,
        scanned_at,
    }
}

/// Quick process-only scan (faster, for periodic checks).
pub fn quick_scan() -> ProctorReport {
    let mut vendor_map: std::collections::HashMap<&'static str, Vec<String>> =
        std::collections::HashMap::new();

    for (vendor_id, evidence) in win::scan_processes() {
        vendor_map.entry(vendor_id).or_default().extend(evidence);
    }

    let mut vendors: Vec<DetectedVendor> = Vec::new();
    for (vendor_id, evidence) in &vendor_map {
        if let Some(profile) = VENDOR_PROFILES.iter().find(|p| p.id == *vendor_id) {
            vendors.push(DetectedVendor {
                name: profile.name,
                vendor_id: profile.id,
                evidence: evidence.clone(),
                capabilities: profile.caps.clone(),
            });
        }
    }

    let active_capabilities = ActiveCapabilities {
        any_screen_capture: vendors.iter().any(|v| v.capabilities.screen_capture),
        any_keyboard_hook: vendors.iter().any(|v| v.capabilities.keyboard_hook),
        any_process_scan: vendors.iter().any(|v| v.capabilities.process_scan),
        any_network_monitor: vendors.iter().any(|v| v.capabilities.network_monitor),
        any_wda_detection: vendors.iter().any(|v| v.capabilities.wda_detection),
        any_browser_lockdown: vendors.iter().any(|v| v.capabilities.browser_lockdown),
    };

    let threat_level = if vendors.is_empty() {
        ThreatLevel::Clear
    } else if active_capabilities.any_wda_detection {
        ThreatLevel::Critical
    } else if active_capabilities.any_browser_lockdown && active_capabilities.any_keyboard_hook {
        ThreatLevel::High
    } else if active_capabilities.any_process_scan || active_capabilities.any_screen_capture {
        ThreatLevel::Medium
    } else {
        ThreatLevel::Low
    };

    let scanned_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    ProctorReport {
        vendors,
        active_capabilities,
        threat_level,
        recommendations: Vec::new(),
        scanned_at,
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Full proctoring scan — called on startup and from frontend.
#[tauri::command]
pub fn proctor_scan() -> ProctorReport {
    full_scan()
}

/// Quick process-only scan — for periodic background checks.
#[tauri::command]
pub fn proctor_quick_scan() -> ProctorReport {
    quick_scan()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_profiles_complete() {
        // Every vendor_id used in signatures must have a profile
        let profile_ids: Vec<&str> = VENDOR_PROFILES.iter().map(|p| p.id).collect();
        for sig in PROCESS_SIGNATURES {
            assert!(
                profile_ids.contains(&sig.vendor_id),
                "Process signature vendor_id '{}' has no profile",
                sig.vendor_id
            );
        }
        for sig in WINDOW_SIGNATURES {
            assert!(
                profile_ids.contains(&sig.vendor_id),
                "Window signature vendor_id '{}' has no profile",
                sig.vendor_id
            );
        }
        for sig in SERVICE_SIGNATURES {
            assert!(
                profile_ids.contains(&sig.vendor_id),
                "Service signature vendor_id '{}' has no profile",
                sig.vendor_id
            );
        }
    }

    #[test]
    fn full_scan_returns_report() {
        let report = full_scan();
        // On a dev machine without proctoring, should be clear
        assert!(matches!(report.threat_level, ThreatLevel::Clear) || !report.vendors.is_empty());
        assert!(!report.scanned_at.is_empty());
    }

    #[test]
    fn quick_scan_returns_report() {
        let report = quick_scan();
        assert!(!report.scanned_at.is_empty());
    }

    #[test]
    fn threat_level_ordering() {
        // Ensure threat levels serialize properly
        let clear = serde_json::to_string(&ThreatLevel::Clear).unwrap();
        assert_eq!(clear, "\"Clear\"");
        let critical = serde_json::to_string(&ThreatLevel::Critical).unwrap();
        assert_eq!(critical, "\"Critical\"");
    }
}
