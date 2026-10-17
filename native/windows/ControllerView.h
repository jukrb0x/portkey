#pragma once
#include "pch.h"
#include <shellapi.h>
#include <vector>
struct ControllerView {
    ControllerView();
    ~ControllerView();

  private:
    void *controller{};
    winrt::Microsoft::UI::Xaml::Window window{nullptr};
    winrt::Microsoft::UI::Xaml::FrameworkElement root{nullptr};
    winrt::Microsoft::UI::Dispatching::DispatcherQueueTimer timer{nullptr};
    winrt::Windows::Data::Json::JsonObject snapshot;
    winrt::Windows::Data::Json::JsonObject selectedProfile;
    std::vector<winrt::Windows::Data::Json::JsonObject> displays, profiles;
    HWND hwnd{};
    NOTIFYICONDATAW tray{};
    std::vector<HDEVNOTIFY> notifications;
    bool updating{}, zh{}, draftLoaded{}, quitting{}, smokeDraftPreserved{}, smokeChinese{},
        smokeCaptureComplete{}, smokeEvidenceCleared{}, smokeInvalidDraftRetained{}, trayReady{},
        editorPending{}, editorRestored{};
    UINT taskbarCreated{};
    unsigned ticks{}, smokeTicks{};
    int setupStep{};
    std::wstring lastSetupPhase;
    template <typename T> T control(wchar_t const *name) { return root.FindName(name).as<T>(); }
    winrt::Windows::Data::Json::JsonObject response(char *value);
    winrt::Windows::Data::Json::JsonObject send(winrt::Windows::Data::Json::JsonObject const &value,
                                                bool showError = true);
    bool action(wchar_t const *type);
    void tick();
    void bind();
    void translate();
    void page(wchar_t const *name);
    void text(wchar_t const *name, winrt::hstring const &value);
    void label(wchar_t const *name, wchar_t const *en, wchar_t const *cn);
    bool setupNames(bool advance = true);
    void finishSetup(bool enable);
    std::function<winrt::Windows::Data::Json::JsonObject(
        winrt::Windows::Data::Json::JsonObject const &)>
        actionOverride;
    winrt::Windows::Data::Json::JsonObject
    workspaceFromControls(winrt::Windows::Data::Json::JsonObject const &source, bool saved = false);
    void renderSetupProgress(winrt::Windows::Data::Json::JsonObject const &setup);
    void setupCompleted(winrt::Windows::Data::Json::JsonObject const &response);
    void orchestrationTests();
    bool startupBlocked{}, blockedSmoke{}, smokeBlockedDiagnostics{}, smokeFinishFlow{},
        restoreSmoke{}, smokeReconstruction{}, smokeReset{}, smokeModes{}, smokeAsyncRecovery{},
        smokeDiagnostics{};
    void clearAttestations(bool clearPeer = true);
    bool setupAction(wchar_t const *type);
    void reconcileSetupEvidence(winrt::Windows::Data::Json::JsonObject const &before,
                                winrt::Windows::Data::Json::JsonObject const &after);
    void loadDraft();
    void profileSelected();
    void saveViewedProfile();
    void useViewedProfile();
    void restoreAutomaticProfile();
    void renderProfileSelection();
    void updateProfiles(winrt::Windows::Data::Json::JsonObject const &previous);
    void profileOrchestrationTests();
    bool smokeProfiles{}, smokeSettings{}, profilePromptOpen{};
    winrt::hstring editorProfileId;
    void qualify();
    void beginSetup();
    winrt::fire_and_forget beginNewProfile();
    void finishLater();
    void resetEditor();
    bool namesValid();
    bool editorHasContent();
    bool persistEditor();
    bool restoreEditor();
    std::filesystem::path editorPath();
    winrt::Windows::Data::Json::JsonObject editorState();
    std::string lastEditorSaved;
    void show();
    void quit();
    void trayMenu();
    void registerNotifications();
    void smokeRecord();
    winrt::fire_and_forget captureWindow(wchar_t const *filename = nullptr);
    static LRESULT CALLBACK windowProc(HWND, UINT, WPARAM, LPARAM, UINT_PTR, DWORD_PTR);
};
