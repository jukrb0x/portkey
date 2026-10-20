#!/usr/bin/env python3
"""Exercise release guards and archive/checksum contents without native binaries."""
import contextlib
import hashlib
import importlib.util
import io
import pathlib
import plistlib
import sys
import tempfile
import unittest.mock
import zipfile

spec = importlib.util.spec_from_file_location('release_package', pathlib.Path(__file__).with_name('release-package.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
assert module.version() == '1.0.3'
with unittest.mock.patch.object(sys, 'argv', ['release-package.py', '--tag', 'v1.0.4']), contextlib.redirect_stderr(io.StringIO()):
    try:
        module.main()
        raise AssertionError('Mismatched tag accepted')
    except SystemExit as error:
        assert error.code == 2
with tempfile.TemporaryDirectory() as temporary:
    root = pathlib.Path(temporary)
    package = root / 'dist/PortKey-Windows'
    package.mkdir(parents=True)
    (package / 'PortKey.exe').write_bytes(b'test fixture, not an executable')
    (package / 'Licenses').mkdir()
    (package / 'Licenses/notice.txt').write_text('license fixture')
    with unittest.mock.patch.object(module, 'ROOT', root), unittest.mock.patch.object(module, 'validate_windows') as validate_windows, unittest.mock.patch.object(module, 'version', return_value='1.0.3'), unittest.mock.patch.object(sys, 'argv', ['release-package.py', '--platform', 'windows-x64']), contextlib.redirect_stdout(io.StringIO()):
        module.main()
    validate_windows.assert_called_once_with(package, '1.0.3')
    archive = next((root / 'dist/release').glob('*.zip'))
    with zipfile.ZipFile(archive) as source:
        assert source.namelist() == ['PortKey-Windows/Licenses/notice.txt', 'PortKey-Windows/PortKey.exe']
    assert archive.with_suffix('.zip.sha256').read_text() == f'{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n'
with tempfile.TemporaryDirectory() as temporary:
    root = pathlib.Path(temporary)
    source = root / 'PortKey-RC.app'
    (source / 'Contents').mkdir(parents=True)
    info = source / 'Contents/Info.plist'
    info.write_bytes(plistlib.dumps({'PortKeyReleaseVersion': '0.2.0'}))
    arguments = ['release-package.py', '--platform', 'macos-arm64', '--source', str(source)]
    with unittest.mock.patch.object(module, 'ROOT', root), unittest.mock.patch.object(module, 'version', return_value='1.0.3'), unittest.mock.patch.object(sys, 'platform', 'darwin'), unittest.mock.patch.object(sys, 'argv', arguments), unittest.mock.patch.object(module.subprocess, 'run') as run, contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()):
        try:
            module.main()
            raise AssertionError('Stale macOS bundle accepted')
        except SystemExit as error:
            assert error.code == 2
        run.assert_not_called()
        info.write_bytes(plistlib.dumps({'PortKeyReleaseVersion': '1.0.3'}))
        def fake_native(command, **kwargs):
            if '-c' in command:
                pathlib.Path(command[-1]).write_bytes(b'archive fixture')
        run.side_effect = fake_native
        module.main()
        assert run.call_args_list[1].args[0][1] == str(source)
        assert pathlib.Path(run.call_args_list[1].args[0][2]).name == 'PortKey.app'
        assert pathlib.Path(run.call_args_list[2].args[0][-2]).name == 'PortKey.app'
with unittest.mock.patch.object(sys, 'platform', 'win32'), unittest.mock.patch.object(module.subprocess, 'run') as run:
    run.return_value.stdout = '0.2.0\n'
    try:
        module.validate_windows(pathlib.Path('staged'), '1.0.3')
        raise AssertionError('Stale Windows package accepted')
    except ValueError:
        pass
    assert run.call_count == 1
    run.reset_mock()
    run.return_value.stdout = '1.0.3\n'
    module.validate_windows(pathlib.Path('staged'), '1.0.3')
    assert run.call_count == 2
    assert 'test-windows-package.ps1' in run.call_args.args[0][4]
print('Release metadata, mismatch rejection, package paths, licenses and checksum checks passed')
