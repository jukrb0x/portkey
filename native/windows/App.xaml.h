#pragma once
#include "App.xaml.g.h"
#include "ControllerView.h"
namespace winrt::PortKey::implementation {
struct App : AppT<App> {
    App();
    void OnLaunched(Microsoft::UI::Xaml::LaunchActivatedEventArgs const &);
    std::unique_ptr<ControllerView> view;
};
} // namespace winrt::PortKey::implementation
