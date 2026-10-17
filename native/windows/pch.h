#pragma once
#include <windows.h>
#include <unknwn.h>
#undef GetCurrentTime
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Data.Json.h>
#include <winrt/Windows.ApplicationModel.DataTransfer.h>
#include <winrt/Microsoft.UI.Xaml.h>
#include <winrt/Microsoft.UI.Xaml.Controls.h>
#include <winrt/Microsoft.UI.Xaml.Controls.Primitives.h>
#include <winrt/Microsoft.UI.Xaml.Markup.h>
#include <winrt/Microsoft.UI.Xaml.Media.h>
#include <winrt/Microsoft.UI.Dispatching.h>
#include <winrt/Microsoft.UI.Windowing.h>
#include <winrt/Microsoft.UI.h>
#include <memory>
#include <string>
#include <functional>
#include <tuple>
#include <algorithm>

#include <winrt/Microsoft.UI.Xaml.Media.Imaging.h>
#include <winrt/Windows.Graphics.Imaging.h>
#include <winrt/Windows.Storage.h>
#include <winrt/Windows.Storage.Streams.h>

#include <fstream>
#include <filesystem>
inline void portkeySmokeTrace(char const *stage) {
    wchar_t value[32768];
    auto n = GetEnvironmentVariableW(L"PORTKEY_SMOKE_RESULT", value, 32768);
    if (n && n < 32768) {
        std::ofstream file(std::filesystem::path(std::wstring(value) + L".trace"), std::ios::app);
        file << stage << "\n";
    }
}
