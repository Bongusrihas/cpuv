"""Package-agnostic adapter for project-declared, immutable source snapshots."""
import json
import os
from conan import ConanFile
from conan.tools.files import copy


class DeclaredSource(ConanFile):
    settings = "os", "arch", "compiler", "build_type"
    options = {"shared": [True, False], "fPIC": [True, False]}
    default_options = {"shared": False, "fPIC": True}
    exports = "recipe.json"
    exports_sources = "source/*"

    def _data(self):
        with open(os.path.join(self.recipe_folder, "recipe.json"), encoding="utf-8") as stream:
            return json.load(stream)

    def requirements(self):
        for requirement in self._data()["requires"]:
            self.requires(requirement)

    def build_requirements(self):
        for requirement in self._data()["spec"]["build"]["tools"]:
            self.tool_requires(requirement)

    def layout(self):
        self.folders.source = "source"
        self.folders.build = "build"
        self.folders.generators = "generators"

    def generate(self):
        build = self._data()["spec"]["build"]
        system = build["system"]
        if system == "cmake":
            from conan.tools.cmake import CMakeDeps, CMakeToolchain
            CMakeDeps(self).generate()
            toolchain = CMakeToolchain(self)
            toolchain.variables.update(build["variables"])
            toolchain.generate()
        elif system == "meson":
            from conan.tools.meson import MesonToolchain
            from conan.tools.gnu import PkgConfigDeps
            PkgConfigDeps(self).generate()
            toolchain = MesonToolchain(self)
            toolchain.project_options.update(build["variables"])
            toolchain.generate()
        elif system == "make":
            from conan.tools.gnu import AutotoolsToolchain
            AutotoolsToolchain(self).generate()
        elif system == "msbuild":
            from conan.tools.microsoft import MSBuildToolchain, MSBuildDeps
            MSBuildToolchain(self).generate()
            MSBuildDeps(self).generate()
        from conan.tools.env import VirtualBuildEnv
        VirtualBuildEnv(self).generate()

    def build(self):
        build = self._data()["spec"]["build"]
        system = build["system"]
        if system == "cmake":
            from conan.tools.cmake import CMake
            tool = CMake(self)
            tool.configure(cli_args=build["arguments"])
            tool.build()
        elif system == "meson":
            from conan.tools.meson import Meson
            tool = Meson(self)
            tool.configure()
            for target in build["arguments"] or [None]:
                tool.build(target=target)
        elif system == "msbuild":
            from conan.tools.microsoft import MSBuild
            copy(self, "*", src=self.source_folder, dst=self.build_folder)
            from xml.sax.saxutils import quoteattr
            imports = os.path.join(self.generators_folder, "cpuv-imports.props")
            with open(imports, "w", encoding="utf-8") as stream:
                stream.write('<Project xmlns="http://schemas.microsoft.com/developer/msbuild/2003">')
                for name in ("conantoolchain.props", "conandeps.props"):
                    stream.write('<Import Project=' + quoteattr(os.path.join(self.generators_folder, name)) + '/>')
                stream.write('</Project>')
            executables = self.conf.get("tools.build:compiler_executables", default={})
            compiler = executables.get("cpp")
            command = MSBuild(self).command(os.path.join(self.build_folder, build["project"]))
            command += ' /p:ForceImportBeforeCppTargets="' + imports + '"'
            command += ' /p:PlatformToolset=' + self._data()["msvc_toolset"]
            if compiler:
                command += ' /p:CLToolPath="' + os.path.dirname(compiler) + '"'
                command += ' /p:CLToolExe="' + os.path.basename(compiler) + '"'
            toolset = os.environ.get("VCTOOLSINSTALLDIR")
            if toolset:
                command += ' /p:VCToolsVersion="' + os.path.basename(toolset.rstrip("/\\")) + '"'
            sdk = os.environ.get("WINDOWSSDKVERSION")
            if sdk:
                command += ' /p:WindowsTargetPlatformVersion="' + sdk.rstrip("/\\") + '"'
            for name, value in build["variables"].items():
                command += ' /p:' + name + '="' + value.replace('"', '\\"') + '"'
            import subprocess
            command += ' ' + subprocess.list2cmdline(build["arguments"])
            self.run(command)
        elif system == "make":
            from conan.tools.gnu import Autotools
            copy(self, "*", src=self.source_folder, dst=self.build_folder)
            Autotools(self).make(args=[name+"="+value for name,value in build["variables"].items()]+build["arguments"])

    def package(self):
        spec = self._data()["spec"]
        build = spec["build"]
        system = build["system"]
        if system == "cmake":
            from conan.tools.cmake import CMake
            CMake(self).install()
        elif system == "meson":
            from conan.tools.meson import Meson
            Meson(self).install()
        elif system == "copy":
            copy(self, "*", src=self.source_folder, dst=self.package_folder)
        for source, destination in build["install"].items():
            # Paths are validated before export. The source/build prefix is explicit.
            root, _, relative = source.replace("\\", "/").partition("/")
            base = self.build_folder if root == "build" else self.source_folder
            if root not in ("source", "build"):
                relative = source
            candidate = os.path.join(base, relative)
            target = os.path.join(self.package_folder, destination)
            if os.path.isdir(candidate):
                copy(self, "*", src=candidate, dst=target)
            else:
                os.makedirs(os.path.dirname(target), exist_ok=True)
                import shutil
                shutil.copyfile(candidate, target)
        for license_file in spec["license_files"]:
            copy(self, os.path.basename(license_file),
                 src=os.path.join(self.source_folder, os.path.dirname(license_file)),
                 dst=os.path.join(self.package_folder, "licenses"), keep_path=False)

    def package_info(self):
        interface = self._data()["spec"]["interface"]
        self.cpp_info.libs = interface["libraries"]
        self.cpp_info.system_libs = interface["system_libraries"]
        self.cpp_info.defines = interface["defines"]
        self.cpp_info.includedirs = interface["include_dirs"] or (["include"] if os.path.isdir(os.path.join(self.package_folder, "include")) else [])
        self.cpp_info.libdirs = interface["library_dirs"] or (["lib"] if os.path.isdir(os.path.join(self.package_folder, "lib")) else [])
        if interface["cmake_file"]:
            self.cpp_info.set_property("cmake_file_name", interface["cmake_file"])
        if interface["cmake_target"]:
            self.cpp_info.set_property("cmake_target_name", interface["cmake_target"])
