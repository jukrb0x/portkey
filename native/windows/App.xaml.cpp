#include "pch.h"
#include "App.xaml.h"
#include <filesystem>
#include <fstream>
#include <MddBootstrap.h>
#include <WindowsAppSDK-VersionInfo.h>
namespace {
void launchError(winrt::hstring const &message) {
    portkeySmokeTrace("launch_error");
    if (std::wstring(GetCommandLineW()).find(L"--smoke-test") != std::wstring::npos) {
        wchar_t path[32768];
        auto n = GetEnvironmentVariableW(L"PORTKEY_SMOKE_RESULT", path, 32768);
        if (n && n < 32768) {
            winrt::Windows::Data::Json::JsonObject error;
            error.SetNamedValue(L"error",
                                winrt::Windows::Data::Json::JsonValue::CreateStringValue(message));
            std::ofstream file(std::filesystem::path(path), std::ios::binary);
            file << winrt::to_string(error.Stringify());
        }
    } else
        MessageBoxW(nullptr, message.c_str(), L"PortKey", MB_OK | MB_ICONERROR);
}
} // namespace
namespace winrt::PortKey::implementation {
App::App() {
    portkeySmokeTrace("app_initialize");
    InitializeComponent();
    portkeySmokeTrace("app_initialized");
}
void App::OnLaunched(Microsoft::UI::Xaml::LaunchActivatedEventArgs const &) {
    try {
        portkeySmokeTrace("on_launched");
        view = std::make_unique<ControllerView>();
        portkeySmokeTrace("view_created");
    } catch (hresult_error const &error) {
        launchError(error.message());
        Exit();
    } catch (std::exception const &error) {
        launchError(to_hstring(error.what()));
        Exit();
    }
}
} // namespace winrt::PortKey::implementation
int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int) {
    try {
        portkeySmokeTrace("main");
        winrt::init_apartment(winrt::apartment_type::single_threaded);
        PACKAGE_VERSION minimum{WINDOWSAPPSDK_RUNTIME_VERSION_UINT64};
        auto bootstrap = MddBootstrapInitialize2(WINDOWSAPPSDK_RELEASE_MAJORMINOR,
                                                 WINDOWSAPPSDK_RELEASE_VERSION_TAG_W, minimum,
                                                 MddBootstrapInitializeOptions_None);
        if (FAILED(bootstrap)) {
            launchError(L"PortKey requires the Microsoft Windows App SDK 2.5.1 x64 runtime, "
                        L"including its DDLM package. Run Install-WindowsRuntime.ps1 from the app "
                        L"folder. Startup stopped safely. Error: " +
                        winrt::to_hstring(static_cast<uint32_t>(bootstrap)));
            return 1;
        }
        portkeySmokeTrace("runtime_initialized");
        winrt::Microsoft::UI::Xaml::Application::Start(
            [](auto const &) { winrt::make<winrt::PortKey::implementation::App>(); });
        MddBootstrapShutdown();
        return 0;
    } catch (winrt::hresult_error const &error) {
        launchError(error.message());
        return 1;
    }
}
