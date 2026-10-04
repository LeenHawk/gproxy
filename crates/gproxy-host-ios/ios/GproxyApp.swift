import SwiftUI
import WebKit

final class AppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        SessionController.shared.register()
        return true
    }
}

@main
struct GproxyApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    var body: some Scene { WindowGroup { SessionView() } }
}

struct SessionView: View {
    @ObservedObject private var session = SessionController.shared
    @State private var duration = 10000
    @State private var showConsole = false

    var body: some View {
        NavigationStack {
            Form {
                Section("本地代理会话") {
                    Text(session.message)
                    Picker("时长上限", selection: $duration) {
                        Text("15 分钟").tag(900)
                        Text("1 小时").tag(3600)
                        Text("10000 秒").tag(10000)
                    }.disabled(session.active || session.busy)
                    if let deadline = session.deadline {
                        HStack { Text("剩余时间"); Spacer(); Text(timerInterval: Date()...deadline, countsDown: true) }
                    }
                    Text("已收到 \(session.requests) 个代理请求")
                    if session.active || session.busy {
                        Button("停止会话", role: .destructive) { session.stop() }.disabled(!session.canStop)
                    } else {
                        Button("开启会话") { session.begin(duration: duration) }.disabled(session.busy)
                    }
                }
                Section("客户端配置") {
                    Text(session.baseURL).textSelection(.enabled)
                    if !session.apiKey.isEmpty {
                        Button("复制 API Key") { UIPasteboard.general.string = session.apiKey }
                    }
                }
                if session.active {
                    Section("管理") {
                        Text("Console 用户名：admin")
                        Button("复制 Console 密码") { UIPasteboard.general.string = session.password }
                        Button("打开 Console") { showConsole = true }
                    }
                }
                Section {
                    Text("用户主动开启后，可切换到其他 App 调用本地代理。系统可能提前结束后台任务，时长上限不代表保证运行时长。")
                }
            }
            .navigationTitle("GPROXY")
            .sheet(isPresented: $showConsole) {
                NavigationStack {
                    ConsoleView(url: URL(string: session.baseURL + "/console/")!)
                        .navigationTitle("Console")
                        .toolbar { Button("关闭") { showConsole = false } }
                }
            }
            .onChange(of: session.active) { _, active in if !active { showConsole = false } }
        }
    }
}

struct ConsoleView: UIViewRepresentable {
    let url: URL
    func makeUIView(context: Context) -> WKWebView {
        let view = WKWebView()
        view.load(URLRequest(url: url))
        return view
    }
    func updateUIView(_ uiView: WKWebView, context: Context) {}
}
