cmake_minimum_required(VERSION 3.14)
project(TestimonialUiPlugin LANGUAGES CXX)

if(DEFINED ENV{LOGOS_MODULE_BUILDER_ROOT})
    include($ENV{LOGOS_MODULE_BUILDER_ROOT}/cmake/LogosModule.cmake)
else()
    message(FATAL_ERROR "LogosModule.cmake not found. Set LOGOS_MODULE_BUILDER_ROOT.")
endif()

file(READ "${CMAKE_CURRENT_SOURCE_DIR}/metadata.json" METADATA_JSON)
string(JSON MODULE_NAME GET ${METADATA_JSON} name)

# The backend makes plain sequencer reads for the view, which has no network.
find_package(Qt6 REQUIRED COMPONENTS Network)

logos_module(
    NAME ${MODULE_NAME}
    REP_FILE src/testimonial_ui.rep
    SOURCES
        src/testimonial_ui_backend.h
        src/testimonial_ui_backend.cpp
    INCLUDE_DIRS
        src
    LINK_LIBRARIES
        Qt6::Network
)
