# Third-party provenance

Selected algorithms and parsing flows were adapted from the pinned calamine
source listed in [ports.json](ports.json). Original notices are retained in the
adapted source files, and the MIT license is distributed under licenses/.

calamine and rust_xlsxwriter are not runtime dependencies or wholesale vendored
libraries. Selected rust_xlsxwriter scalar/formula/style XML layouts, date conversion, sequential spooling and packaging flows are now adapted into the XLSX writer. Its MIT license is distributed under licenses/ and notices are retained in adapted source files. Full upstream model/style/feature imports remain deferred.
