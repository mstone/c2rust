#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import subprocess
import os
import json
import logging
import argparse
import tempfile
import transpile
from shutil import copyfile
import common

csmith_home = "/usr/local/opt/csmith/include/csmith-2.3.0/runtime"
csmith_cmd = ["csmith", "--no-bitfields", "--no-builtins"]
c_compiler = "clang"
rust_compiler = "rustc"
csmith_timeout = 5 # seconds to wait for C compiled executable to run

def create_compile_commands(dirname, output_c_name):
    """Create a compile commands file suitable for compiling the given csmith source file."""

    compile_commands_settings = [{
        'directory': dirname,
        'arguments':
            [c_compiler,
             "-D_FORTIFY_SOURCE=0",
             "-isystem", "/usr/include",
             "-I", csmith_home,
             output_c_name],
        'file': output_c_name }]

    compile_commands_name = os.path.join(dirname, 'compile_commands.json')
    with open(compile_commands_name, 'w') as file:
        file.write(common.json_pp_obj(compile_commands_settings))
    
    return compile_commands_name

def generate_c_source(output_c_name):
    """Generate a C source file using csmith."""

    with open(output_c_name, 'w') as output_c:
        logging.info("Generating C source file with csmith")
        subprocess.run(csmith_cmd, stdout=output_c, check=True)

def transpile_file(dirname, output_c_name):
    compile_commands_name = create_compile_commands(dirname, output_c_name)
    with open(compile_commands_name) as compile_commands:
        transpile.transpile_files(
            compile_commands,
            None, #filter
            [], #extra impo args
            False, #import_only
            False, # verbose
            False, # emit_build_files
        )
                        
def compile_c_file(output_c_name, output_c_exe_name):
    """Compile the given C source file to produce the given executable."""

    logging.info("Compiling C source file with clang")
    compile_cmd = [
        c_compiler,
        "-I", csmith_home,
        "-o", output_c_exe_name,
        output_c_name]
    subprocess.run(
        compile_cmd,
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL)

def execute_c_driver(output_c_exe_name):
    """Execute the given executable and return its stdout output."""

    logging.info("Executing csmith driver")
    exec_c_result = subprocess.run(
        output_c_exe_name,
        capture_output=True,
        check=True,
        timeout=csmith_timeout)
    expected_output = exec_c_result.stdout
    logging.info("Execution finished: %s", expected_output)
    return expected_output

def compile_rust_file(dirname, output_c_name, output_rs_name, output_rs_exec_name):
    """Compile the given Rust source file."""

    logging.info("Compiling translated Rust")
    compile_rust_cmd = [rust_compiler, '-Awarnings', output_rs_name, '-o', output_rs_exec_name]
    try:
        subprocess.run(compile_rust_cmd, check=True)
    except:
        logging.info("Compile failure, saving source files locally")
        copyfile(output_c_name, 'output.c')
        copyfile(output_rs_name, 'output.rs')
        raise

def execute_rust_driver(output_rs_exec_name):
    """Given the filename of the Rust executable, execute it and record its stdout output."""

    logging.info("Executing csmith rust driver")
    exec_rs_result = subprocess.run(output_rs_exec_name, capture_output=True, check=True)
    actual_output = exec_rs_result.stdout
    logging.info("Execution finished: %s", actual_output)
    return actual_output

def main():
    common.setup_logging()

    with tempfile.TemporaryDirectory('_c2rust_csmith') as dirname:

        # generate filenames based on tempdir
        output_c_name = os.path.join(dirname, 'output.c')
        output_c_exe_name = os.path.join(dirname, 'output.c.exe')
        output_rs_name = os.path.join(dirname, 'output.rs')
        output_rs_exec_name = os.path.join(dirname, 'output.rs.exe')

        logging.info("Using temporary directory: %s", dirname)

        # Generate and run C version
        generate_c_source(output_c_name)  
        compile_c_file(output_c_name, output_c_exe_name)
        expected_output = execute_c_driver(output_c_exe_name)

        # Generate and run Rust version
        transpile_file(dirname, output_c_name)
        compile_rust_file(dirname, output_c_name, output_rs_name, output_rs_exec_name)
        actual_output = execute_rust_driver(output_rs_exec_name)

        if expected_output == actual_output:
            logging.info("Match")
        else:
            logging.info("FAILURE: %s %s", expected_output, actual_output)
            copyfile(output_c_name, 'output.c')
            copyfile(output_rs_name, 'output.rs')


if __name__ == "__main__":
    main()