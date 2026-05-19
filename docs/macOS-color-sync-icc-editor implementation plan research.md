根据提供的资料，在macOS的ColorSync架构中，显示器（Monitor）的ICC配置文件是一个二进制文件，采用严格的大端序（big-endian）来表示所有多字节整数和定点数 1。显示器配置文件通常属于“矩阵/整形器”（matrix/shaper）或“矩阵/1D LUT”模型 1, 2。

该ICC文件的内部结构主要由三个部分组成：**128字节的文件头（Header）**、**标签目录（Tag directory）以及包含实际色彩转换信息的数据段（Data segment）** 1。

以下是其具体的文件结构解释：

### 1. 128字节文件头（Header）

文件头包含固定大小的元数据，用于色彩管理模块（CMM）识别配置文件的基本属性和用途。针对macOS的显示器配置，关键字段及其字节偏移量包括 1：

- **0-3字节**：Profile Size（配置文件大小），即文件的总字节长度。
- **4-7字节**：CMM Type（CMM类型），首选CMM的签名（例如Apple环境通常为 appl）。
- **8-11字节**：Profile Version（配置文件版本），例如v2.1版本表示为 0x02100000。
- **12-15字节**：Profile Class（配置文件类别），**对于显示器而言该值必须是 mntr**。
- **16-19字节**：Color Space（色彩空间），屏幕通常为 RGB 。
- **20-23字节**：PCS（Profile Connection Space，配置文件连接空间），通常为 XYZ 或 Lab 。
- **36-39字节**：Magic Number（魔数），始终为 acsp，这是有效ICC文件的明确标识。
- **40-43字节**：Platform（平台），主要平台标识（例如Apple为 APPL）。
- **68-79字节**：Illuminant（光源），PCS标准光源，固定为D50。
- **84-99字节**：Profile ID，配置文件内容的MD5哈希值（在ICC v4版本中为强制要求），用于系统验证文件完整性。

### 2. 标签目录与数据对齐（Tag Directory and Data Alignment）

紧接在128字节文件头之后的是标签总数以及标签表（目录）。

- **目录条目**：每个标签条目长度为12个字节，由4字节的签名（signature）、4字节的绝对偏移量（offset，从文件头开始计算）和4字节的数据长度（length）组成 1。
- **对齐要求**：一个关键的架构要求是**所有标签数据必须在4字节边界上对齐**。如果数据长度不满足要求，必须插入填充字节（通常是空字节），以确保CMM能在各种硬件架构上高效读取 1。

### 3. 显示器配置文件的核心特征标签（Essential Tags）

数据段存放了目录中所指向的标签数据。一个典型的macOS显示器ICC配置文件主要包含以下描述显示器色彩响应的参数 1：

- **wtpt（白点, White Point）**：指定适配到D50的显示器介质白点。
- **rXYZ, gXYZ, bXYZ（主要着色剂）**：定义显示器在最高亮度下红、绿、蓝三原色的相对XYZ坐标。
- **rTRC, gTRC, bTRC（色调再现曲线）**：定义红绿蓝各通道的伽马（Gamma）或响应曲线。ColorSync系统利用这些曲线在应用色彩矩阵前将RGB值线性化。
- **desc（描述）**：包含人类可读的配置文件名称，即在macOS“系统偏好设置 -> 显示器”列表中呈现的名称。
- **vcgt（显卡伽马表, Video Card Gamma Table）**：这是一个**Apple特有的扩展标签**。与软件层面的TRC转换不同，vcgt 允许将校准数据直接加载到显卡硬件的查找表（GPU hardware LUT）中。这使得亮度和色温等全局修改不仅作用于色彩管理应用，还会影响整个系统桌面。该标签内的数据可以存储为数学公式格式或离散的表格格式（例如常用的3通道、每通道256个条目的8位表） 1。

总结来说，macOS通过Color Sync API处理的显示器ICC文件，结构上遵循国际色彩联盟的规范，并通过内置的 vcgt 标签实现了软硬件结合的底层色彩与亮度校准 1。

1, 2. https://argyllcms.com/doc/Scenarios.html



Architectural Integration of the macOS ColorSync Video Card Gamma Tag and Core Graphics Transfer Functions

The modern computational display pipeline on macOS represents a convergence of high-level colorimetric description and low-level hardware control. This integration is primarily facilitated through the ColorSync framework and the Quartz Display Services within the Core Graphics framework. At the center of this architecture is the Video Card Gamma Tag (vcgt), a private tag structure within International Color Consortium (ICC) profiles that bridges the gap between device characterization and hardware calibration.[1, 2, 3] To understand the mechanism by which macOS ensures color fidelity, it is necessary to perform a rigorous analysis of the binary structure of the `vcgt` tag, the operational logic of the Core Graphics transfer functions, and the functional relationship that transforms static file metadata into a dynamic hardware state.

The Foundations of macOS Color Management and the Role of ColorSync

ColorSync serves as the native color management system (CMS) for macOS, providing an implementation of the ICC specification to ensure consistent color reproduction across diverse imaging devices, including scanners, cameras, monitors, and printers.[4] The fundamental goal of any CMS is to deliver color that accurately represents the original source throughout a digital workflow.[4] In the context of display technology, this involves two distinct but interrelated processes: characterization and calibration.[2, 3, 5]

Characterization, or profiling, is the process of documenting the way a device responds to color signals. The result is stored in an ICC profile, which describes the device's color space relative to a standard Profile Connection Space (PCS), such as CIE Lab or XYZ.[2, 3, 4] An ICC profile, in its standard form, does not modify color by itself; instead, it provides the mathematical primitives—matrices and multi-dimensional lookup tables (CLUTs)—necessary for a Color Management Module (CMM) to perform on-the-fly color conversions.[1, 3, 4]

Calibration, however, is the process of adjusting a device's physical response to a predetermined condition, such as a specific white point, brightness level, or gamma curve.[2, 5] For displays, calibration is often achieved by modifying the Video Card Gamma Table (VCGT), a set of one-dimensional lookup tables (1D LUTs) located in the graphics hardware.[1] These tables intercept the digital color values before they are converted to an analog or digital signal for the panel, allowing for fine-tuned adjustments to the display's tone response and gray balance.[1, 2]

**Apple introduced the** `vcgt` **tag in ColorSync 2.5 to provide a standardized method for storing these calibration parameters directly within the display's ICC profile.[6] This allows the operating system to automatically synchronize the hardware state with the characterization data, ensuring that the profile remains valid for the current state of the monitor.[5] Without the** `vcgt` **tag, a display profile would only be accurate if the user manually restored the hardware settings to the exact state they were in during the profiling session.[1, 5]**

Detailed Anatomy of the Video Card Gamma Tag Structure

The `vcgt` tag is a private extension to the ICC profile format, identified by the four-character signature `vcgt` (0x76636774) in the profile's tag table.[6, 7] The data associated with this tag follows a specific binary layout designed to be parsed by the ColorSync Manager. The tag structure is defined to accommodate two primary methods of defining gamma correction: the formula-based approach and the table-based approach.[6, 8]

The Universal Header and Type Signatures

Regardless of the storage method, all `vcgt` tags begin with a standard header that identifies the tag and the type of data it contains. The ColorSync Manager uses these signatures to determine how to decode the subsequent bytes.[6, 8]

| Field Name     | Data Type | Byte Offset | Value/Description                      |
| -------------- | --------- | ----------- | -------------------------------------- |
| Tag Signature  | uInt32    | 0           | 'vcgt' (0x76636774) [6]                |
| Reserved       | uInt32    | 4           | Set to 0 for future compatibility [9]  |
| Type Signature | uInt32    | 8           | 'frm ' (Formula) or 'tbl ' (Table) [8] |

The type signature acts as a switch for the parser. The constant `cmVideoCardGammaFormulaType` corresponds to the 'frm ' signature, while `cmVideoCardGammaTableType` corresponds to 'tbl '.[8]

Deconstructing the Formula-Based Structure

The formula-based approach, designated by 'frm ', is used when the gamma correction can be represented by a smooth mathematical power-law curve. This method is highly efficient, requiring only a few parameters to describe the entire tone response for each of the red, green, and blue channels.[6, 8]

In this format, each color channel is defined by three parameters: gamma, minimum, and maximum. These values are stored as `u16Fixed16` numbers, which are 32-bit unsigned fixed-point integers where the high 16 bits represent the integer part and the low 16 bits represent the fractional part.[9]

| Parameter | Red Channel (Offset) | Green Channel (Offset) | Blue Channel (Offset) |
| --------- | -------------------- | ---------------------- | --------------------- |
| Gamma     | 12                   | 24                     | 36                    |
| Minimum   | 16                   | 28                     | 40                    |
| Maximum   | 20                   | 32                     | 44                    |

The mathematical model used to generate the discrete 1D LUT values from these parameters is defined by the following formula for each channel:

*f*(*x*)=(*M**a**x*−*M**in*)⋅*x**G**amma*+*M**in*

Where *x* is the normalized input value (0.0≤*x*≤1.0) and *f*(*x*) is the normalized output value. This formula ensures that the resulting curve is continuous and smooth, which is ideal for avoiding quantization artifacts in software-only gamma adjustments.[8]

Deconstructing the Table-Based Structure

The table-based approach, designated by 'tbl ', is the standard for professional-grade display profiles. Unlike the formula type, the table type allows for arbitrary, non-linear adjustments that can correct for specific irregularities in a display's gray scale or white point.[2, 10] This format stores three independent 1D lookup tables, one for each primary color channel.[2, 10]

The structure of a table-based `vcgt` tag includes metadata that describes the dimensions and precision of the tables, followed by the raw table data.[10]

| Field Name       | Data Type | Byte Offset     | Description                                                |
| ---------------- | --------- | --------------- | ---------------------------------------------------------- |
| Channel Count    | uInt16    | 12              | Number of channels (Standardly 3 for RGB) [10]             |
| Entry Count      | uInt16    | 14              | Number of entries in each table (Commonly 256) [10]        |
| Entry Size       | uInt16    | 16              | Size of each entry in bytes (Standardly 2 for 16-bit) [10] |
| Red Table Data   | uInt16    | 18              | Array of size `Entry Count` [10]                           |
| Green Table Data | uInt16    | 18 + (1 * Size) | Array of size `Entry Count` [10]                           |
| Blue Table Data  | uInt16    | 18 + (2 * Size) | Array of size `Entry Count` [10]                           |

The entry size is a critical field. While 8-bit displays traditionally use 256 entries with 1-byte entry sizes, professional ICC profiles almost exclusively use 2-byte (16-bit) entries to maintain high precision.[10] A 16-bit table entry provides a value range from 0 to 65535, which the operating system then maps to the hardware's internal bit depth.[10, 11]

Quartz Display Services and the Core Graphics API

While ColorSync manages the ICC profiles and the extraction of the `vcgt` tag, the actual communication with the graphics hardware is handled by the Quartz Display Services, a sub-framework of Core Graphics.[4, 12, 13] The Quartz Display Services provide two primary functions for interacting with the display's gamma tables: `CGGetDisplayTransferByTable` and `CGSetDisplayTransferByTable`.[11, 12, 13]

CGGetDisplayTransferByTable: Retrieving the Hardware State

The `CGGetDisplayTransferByTable` function allows developers to query the current state of the display's gamma ramps.[13] This is useful for diagnostic tools or for calibration software that needs to verify if its previous settings are still active.[14]

The function signature, as defined in the Core Graphics headers, is:

```
CGError CGGetDisplayTransferByTable(
    CGDirectDisplayID display,
    uint32_t capacity,
    CGGammaValue *redTable,
    CGGammaValue *greenTable,
    CGGammaValue *blueTable,
    uint32_t *sampleCount
);
```

Key characteristics of this function include:

**CGDirectDisplayID**: A unique identifier for the display being accessed.[15]**Capacity**: The maximum number of entries the provided buffers can hold.[13]**CGGammaValue**: A 32-bit floating-point type (`float`) representing a normalized color intensity in the range [0.0, 1.0].[11, 15]**SampleCount**: On return, this points to the number of entries actually copied into the tables.[13]

This function retrieves the values directly from the window server, reflecting the current transformation applied to the video signal. If a profile has been loaded by the system, these values will correspond to the data found in the `vcgt` tag of that profile.[1]

CGSetDisplayTransferByTable: Modifying the Hardware State

The `CGSetDisplayTransferByTable` function is the mechanism by which the operating system or a third-party application applies new calibration curves to the display hardware.[11]

```
CGError CGSetDisplayTransferByTable(
    CGDirectDisplayID display,
    uint32_t tableSize,
    const CGGammaValue *redTable,
    const CGGammaValue *greenTable,
    const CGGammaValue *blueTable
);
```

When this function is called, the window server and the graphics driver take the provided floating-point tables and interpolate them to match the physical requirements of the GPU's Lookup Table (LUT).[11, 12] For example, if a modern GPU uses a 1024-entry internal ramp, but the application provides a 256-entry table, Core Graphics will perform linear interpolation to fill the intermediate values.[12] This abstraction allows software to remain agnostic of the specific hardware bit depth or LUT capacity of the connected monitor.[1, 10]

Functional Relationship and Data Transformation Logic

The relationship between the `vcgt` tag in an ICC file and the Core Graphics transfer functions is a serialized-to-runtime mapping. The `vcgt` tag serves as the persistent storage of the calibration intent, while the Core Graphics functions provide the interface to realize that intent in hardware.[1, 3]

The Mapping Process: From Binary to Floating-Point

When the ColorSync Manager loads an ICC profile for a display, it performs a series of transformations to prepare the data for the Core Graphics API.

**Tag Detection and Validation**: The parser identifies the `vcgt` tag and verifies the signature.[6]**Endian Conversion**: ICC profiles are strictly Big-Endian. Since modern Mac hardware (Intel and Apple Silicon) is Little-Endian, the ColorSync Manager must perform byte-swapping on all 16-bit or 32-bit values read from the tag.[9, 16]**Normalization**:For **Table-type** tags, the integer values (0 to 65535 for 16-bit entries) are divided by the maximum possible value (e.g., 65535.0) to convert them into `CGGammaValue` floats in the range [0.0, 1.0].[10, 11]For **Formula-type** tags, the mathematical formula is evaluated for a standard number of steps (typically 256) to generate three discrete arrays of floating-point values.[6, 8]**Hardware Update**: The resulting three one-dimensional arrays (Red, Green, and Blue) are passed to `CGSetDisplayTransferByTable`.[1, 11]

The Role of 1D Lookup Tables

The "three one-dimensional lookup tables" mentioned in the query are the specific outputs of this process. They are the runtime representation of the `vcgt` data. Each table defines a mapping where the index of the table represents the input color intensity (e.g., the digital signal from the application) and the value at that index represents the output intensity (the signal sent to the display).[1, 5]

| Input Index (Normalized) | Red Value (*R*′) | Green Value (*G*′) | Blue Value (*B*′) |
| ------------------------ | ---------------- | ------------------ | ----------------- |
| 0.000 (Black)            | 0.0000           | 0.0000             | 0.0000            |
| 0.500 (Mid-gray)         | 0.2176           | 0.2176             | 0.2176            |
| 1.000 (White)            | 1.0000           | 1.0000             | 1.0000            |

In a perfectly linear system (Identity LUT), the output value would always equal the input index. The `vcgt` tag allows for "warping" this relationship to achieve a desired gamma (e.g., 2.2) or to correct for a monitor that has a native blue tint.[1, 17]

Architectural Implications and Third-Order Insights

The synergy between ColorSync and Core Graphics provides a robust framework, but it also introduces complexities in modern heterogeneous computing environments. Several high-level insights can be derived from the way these systems interact.

Separation of Calibration and Characterization

A common point of confusion in professional color workflows is the distinction between what the hardware LUT (VCGT) does and what the ICC profile's 3D LUT (CLUT) does.[1] The `vcgt` tag only provides 1D correction. It can adjust the white point, the contrast, and the individual gamma of the three channels, but it cannot correct for "cross-talk" between channels (e.g., if increasing the red signal also slightly increases the green output).[1, 3] Correcting such non-linearities requires the multi-dimensional mapping found in the main tags of the ICC profile (like `AToB0` or `BToA0`), which are processed by the CMM in software, not by the video card hardware.[1, 7]

The Impact of Bit Depth and Quantization

When a 1D LUT is applied via `CGSetDisplayTransferByTable`, the transformation occurs in the display pipeline. If the transformation is drastic—for example, reducing the maximum output of the blue channel to 50% to warm up a very cool display—the effective bit depth of that channel is reduced.[1, 18] If the GPU and the monitor interface (HDMI/DP) are operating at 8-bit, this can lead to visible banding (contouring) in gradients. Modern macOS systems mitigate this by performing internal LUT calculations at 10-bit or 12-bit precision and using dithering at the output stage, even if the source application is only 8-bit.[10, 18]

Apple Silicon and the Evolution of the Video Card Gamma Table

On legacy Intel-based Macs, the VCGT was a literal component of the discrete or integrated GPU's scan-out engine. With the transition to Apple Silicon (M1, M2, M3 series), the display pipeline has been integrated into the System-on-a-Chip (SoC) within a specialized block known as the Display Controller (DCP).[19]

In this modern architecture, the "Video Card Gamma Table" is a logical stage within the DCP's color pipeline. Research indicates that Apple Silicon Macs handle these tables differently, particularly when "Reference Modes" are active.[19, 20] For displays like the Pro Display XDR or the Liquid Retina XDR on MacBook Pro models, selecting a reference preset (such as "Design & Print") may override or bypass the `vcgt` tag entirely, as the hardware is already factory-calibrated to a known state.[19] This can cause frustration for users who attempt to use third-party calibration tools, as the OS may revert any manual changes to the gamma tables to maintain the integrity of the selected reference mode.[19]

Mathematical Interpolation and Performance Optimization

The efficiency of the macOS graphics stack is partly due to the way it handles transfer tables of varying sizes. While the `vcgt` tag might contain 256 entries (8-bit) or 1024 entries (10-bit), the Core Graphics API is designed to handle arbitrary sizes through the `tableSize` parameter.[11, 21]

The interpolation algorithm typically used is a standard linear interpolation between discrete points. If an input value *x* falls between two indices *i* and *i*+1 in the table, the output *y* is calculated as:

*y*=(1−*α*)⋅*T**ab**l**e*[*i*]+*α*⋅*T**ab**l**e*[*i*+1]

Where *α* is the fractional distance between the indices. This calculation is handled by the GPU's texture sampling units or specialized LUT hardware, ensuring that applying a complex `vcgt` curve has zero impact on the system's overall frame rate or UI responsiveness.[12]

Cross-Platform Comparisons: macOS vcgt vs. Windows MHC2

While the `vcgt` tag originated on the Macintosh, its utility led to widespread adoption in the Windows ecosystem, where it is often embedded in `.icm` profiles used by professional photographers and designers.[1, 5] However, Windows does not have a native "ColorSync" system that automatically loads these tags; instead, third-party utilities like "DisplayCAL" or "Adobe Gamma Loader" must be used to read the `vcgt` and call the equivalent Windows GDI function, `SetDeviceGammaRamp`.[5, 22, 23]

More recently, Microsoft introduced the "MHC2" (Microsoft Hardware Calibration) tag, which is a modern, HDR-aware successor to the `vcgt`.[22] The MHC2 tag supports more complex GPU color transform pipelines, including matrix transforms and target re-gamma stages, which are specifically designed for high-dynamic-range (HDR) and Wide Color Gamut (WCG) displays.[22] Despite this, the `vcgt` tag remains the industry standard for SDR (Standard Dynamic Range) calibration due to its simplicity and broad compatibility.[1, 22]

Implementation Details in Third-Party Software

The ubiquity of the `vcgt` tag has made it a primary target for open-source color management libraries. For instance, the Little CMS (lcms2) library provides comprehensive support for reading and writing `vcgt` tags, allowing developers to create cross-platform applications that can interpret macOS display profiles.[24, 25]

The library `xcalib` is a prominent example of a utility that directly interacts with these structures.[5] On macOS, `xcalib` can be used to manually clear the gamma tables or load a specific profile, which is particularly useful for debugging color shifts in video production.[5, 26]

| Utility             | Method of Interaction            | Primary Use Case                          |
| ------------------- | -------------------------------- | ----------------------------------------- |
| ColorSync Utility   | System-level profile association | General OS color management [27]          |
| DisplayCAL          | Generates `vcgt` via measurement | Professional monitor calibration [23]     |
| dispwin (ArgyllCMS) | Loads `vcgt` or `.cal` files     | Command-line calibration loading [14, 28] |
| xcalib              | Direct CGSetDisplayTransfer call | Scripted gamma table manipulation [5]     |

Conflict Resolution and Persistence Issues

A critical area of technical interest is the persistence of `vcgt` settings. In a multi-layered operating system like macOS, multiple processes may attempt to control the display's color state simultaneously.[19]

**Night Shift and True Tone**: These system features dynamically modify the display's white point by injecting their own values into the color pipeline. Research suggests that these features do not necessarily modify the `vcgt` of the active profile, but rather apply a secondary transformation in a different stage of the GPU's color pipeline.[19, 21]**Reference Presets (Apple Silicon)**: On M-series Macs, the system re-applies the reference preset at login and on display hot-plug events. This process can "wipe out" any custom `vcgt` data that was injected by a third-party application, necessitating a re-load of the calibration.[19]**Full-Screen Applications**: Some legacy games or video players that use the "Display Capture" API may override the system gamma ramps, leading to the "gamma shift" problem often discussed by video colorists.[19, 20, 29]

Conclusion: The Functional Synthesis of ColorSync and Core Graphics

The architecture of macOS color management is defined by a clear hierarchy of metadata and hardware control. The `vcgt` tag, through its formulaic ('frm ') or tabular ('tbl ') structures, provides a rigorous method for serializing a display's calibrated state into a standard ICC profile. This metadata is then parsed by the ColorSync Manager, which transforms the Big-Endian integer data into the normalized floating-point format required by the Quartz Display Services.

The relationship between the `vcgt` and the Core Graphics functions `CGGetDisplayTransferByTable` and `CGSetDisplayTransferByTable` is one of implementation. The functions provide the runtime handles for the 1D RGB lookup tables that represent the physical calibration of the display. This system ensures that color management on macOS is not merely a software-level approximation but a hardware-level guarantee of accuracy. As display technology moves toward SoC-integrated controllers and advanced HDR standards, the legacy of the `vcgt` tag continues to serve as the foundation for how the operating system maintains visual fidelity across a diverse and evolving hardware landscape.

\--------------------------------------------------------------------------------

Display Profile - Page 3 - darktable - discuss.pixls.us, [https://discuss.pixls.us/t/display-profile/11781?page=3](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdiscuss.pixls.us%2Ft%2Fdisplay-profile%2F11781%3Fpage%3D3)Customized ICC Display Profile Construction and Concerns, [https://www.printing.org/docs/default-source/taga-abstracts-(member-only)/t100243.pdf?sfvrsn=fe92d1ed_2](https://www.google.com/url?sa=E&q=https%3A%2F%2Fwww.printing.org%2Fdocs%2Fdefault-source%2Ftaga-abstracts-(member-only)%2Ft100243.pdf%3Fsfvrsn%3Dfe92d1ed_2)Calibration vs. Characterization - ArgyllCMS, [https://argyllcms.com/doc/calvschar.html](https://www.google.com/url?sa=E&q=https%3A%2F%2Fargyllcms.com%2Fdoc%2Fcalvschar.html)Technical Note TN2313: Best Practices for Color Management in OS X and iOS, [https://developer.apple.com/library/archive/technotes/tn2313/_index.html](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Flibrary%2Farchive%2Ftechnotes%2Ftn2313%2F_index.html)GitHub - OpenICC/xcalib: Load 'vcgt'-tag of ICC profiles to X-server and MS-Windows. Works on calibration stage, which can be a precondition for display ICC color conversions., [https://github.com/OpenICC/xcalib](https://www.google.com/url?sa=E&q=https%3A%2F%2Fgithub.com%2FOpenICC%2Fxcalib)cmVideoCardGammaTag | Apple Developer Documentation, [https://developer.apple.com/documentation/applicationservices/1560164-video_card_gamma_tags/cmvideocardgammatag](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fapplicationservices%2F1560164-video_card_gamma_tags%2Fcmvideocardgammatag)Insighter: looking inside ICC profiles - LittleCMS, [https://littlecms.com/blog/2026/03/20/insighter/](https://www.google.com/url?sa=E&q=https%3A%2F%2Flittlecms.com%2Fblog%2F2026%2F03%2F20%2Finsighter%2F)Video Card Gamma Storage Types | Apple Developer Documentation, [https://developer.apple.com/documentation/applicationservices/1560344-video_card_gamma_storage_types](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fapplicationservices%2F1560344-video_card_gamma_storage_types)Inside the ICC Color Device Profile - Imaging.org, [https://www.imaging.org/common/uploaded%20files/pdfs/Papers/1998/RP-0-69/2197.pdf](https://www.google.com/url?sa=E&q=https%3A%2F%2Fwww.imaging.org%2Fcommon%2Fuploaded%20files%2Fpdfs%2FPapers%2F1998%2FRP-0-69%2F2197.pdf)Wayland color management - Page 19 - Software - discuss.pixls.us, [https://discuss.pixls.us/t/wayland-color-management/10804?page=19](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdiscuss.pixls.us%2Ft%2Fwayland-color-management%2F10804%3Fpage%3D19)CGSetDisplayTransferByTable(*:*:*:*:_:) | Apple Developer Documentation, [https://developer.apple.com/documentation/coregraphics/cgsetdisplaytransferbytable(](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcgsetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcgsetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcgsetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcgsetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A))[:_:)](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcgsetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A))CGSetDisplayTransferByByteTab... - Apple Developer, [https://developer.apple.com/documentation/coregraphics/cgsetdisplaytransferbybytetable(](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcgsetdisplaytransferbybytetable(_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcgsetdisplaytransferbybytetable(_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcgsetdisplaytransferbybytetable(_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcgsetdisplaytransferbybytetable(_%3A_%3A_%3A_%3A_%3A))[:_:)](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcgsetdisplaytransferbybytetable(_%3A_%3A_%3A_%3A_%3A))CGGetDisplayTransferByTable(*:*:*:*:*:*:) | Apple Developer ..., [https://developer.apple.com/documentation/coregraphics/cggetdisplaytransferbytable(](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcggetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcggetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcggetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcggetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcggetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A_%3A))[:](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcggetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A_%3A))[:)](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcggetdisplaytransferbytable(_%3A_%3A_%3A_%3A_%3A_%3A))ICC Display Profiles with Control over the VCGT, [https://www.color.org/groups/medical/displays/controllingVCGT.pdf](https://www.google.com/url?sa=E&q=https%3A%2F%2Fwww.color.org%2Fgroups%2Fmedical%2Fdisplays%2FcontrollingVCGT.pdf)Core Graphics Data Types | Apple Developer Documentation, [https://developer.apple.com/documentation/coregraphics/core-graphics-data-types](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdeveloper.apple.com%2Fdocumentation%2Fcoregraphics%2Fcore-graphics-data-types)lcms2.h - PDFium, [https://pdfium.googlesource.com/pdfium/+/5110c4743751145c4ae1934cd1d83bc6c55bb43f/core/src/fxcodec/lcms2/lcms2-2.6/include/lcms2.h](https://www.google.com/url?sa=E&q=https%3A%2F%2Fpdfium.googlesource.com%2Fpdfium%2F%2B%2F5110c4743751145c4ae1934cd1d83bc6c55bb43f%2Fcore%2Fsrc%2Ffxcodec%2Flcms2%2Flcms2-2.6%2Finclude%2Flcms2.h)Useful Profiles for Testing vcgt Tags - Bruce Lindbloom, [http://www.brucelindbloom.com/Vcgt.html](https://www.google.com/url?sa=E&q=http%3A%2F%2Fwww.brucelindbloom.com%2FVcgt.html)ColourSpace | Direct Profiling - Light Illusion, [https://lightillusion.com/direct_profiling.html](https://www.google.com/url?sa=E&q=https%3A%2F%2Flightillusion.com%2Fdirect_profiling.html)Apple MacBook Pro М2 2022 os Tahoe 26.1 spontaneous reset of the monitor profile, [https://www.reddit.com/r/colorists/comments/1py2xuu/apple_macbook_pro_%D0%BC2_2022_os_tahoe_261/](https://www.google.com/url?sa=E&q=https%3A%2F%2Fwww.reddit.com%2Fr%2Fcolorists%2Fcomments%2F1py2xuu%2Fapple_macbook_pro_%D0%BC2_2022_os_tahoe_261%2F)grading on MacBook Pro, which display preset+output gamma? - Blackmagic Forum, [https://forum.blackmagicdesign.com/viewtopic.php?f=21&t=227265](https://www.google.com/url?sa=E&q=https%3A%2F%2Fforum.blackmagicdesign.com%2Fviewtopic.php%3Ff%3D21%26t%3D227265)Problems changing the gamma tables in the monitor using CGGetDisplayTransferByTable, [https://stackoverflow.com/questions/62991411/problems-changing-the-gamma-tables-in-the-monitor-using-cggetdisplaytransferbyta](https://www.google.com/url?sa=E&q=https%3A%2F%2Fstackoverflow.com%2Fquestions%2F62991411%2Fproblems-changing-the-gamma-tables-in-the-monitor-using-cggetdisplaytransferbyta)Windows hardware display color calibration pipeline - Win32 apps - Microsoft Learn, [https://learn.microsoft.com/en-us/windows/win32/wcs/display-calibration-mhc](https://www.google.com/url?sa=E&q=https%3A%2F%2Flearn.microsoft.com%2Fen-us%2Fwindows%2Fwin32%2Fwcs%2Fdisplay-calibration-mhc)DisplayCAL—Display Calibration and Characterization powered by ArgyllCMS, [https://displaycal.net/](https://www.google.com/url?sa=E&q=https%3A%2F%2Fdisplaycal.net%2F)Little Color Management System, [https://www.littlecms.com/](https://www.google.com/url?sa=E&q=https%3A%2F%2Fwww.littlecms.com%2F)Color management on Wayland (#11) · Issue · plasma/kwin - KDE Invent, [https://invent.kde.org/plasma/kwin/-/issues/11](https://www.google.com/url?sa=E&q=https%3A%2F%2Finvent.kde.org%2Fplasma%2Fkwin%2F-%2Fissues%2F11)xcalib - Tiny monitor calibration loader for Xorg. - Ubuntu Manpages, [https://manpages.ubuntu.com/manpages/jammy/man1/xcalib.1.html](https://www.google.com/url?sa=E&q=https%3A%2F%2Fmanpages.ubuntu.com%2Fmanpages%2Fjammy%2Fman1%2Fxcalib.1.html)Check and repair color profiles in ColorSync Utility on Mac - Apple Support, [https://support.apple.com/guide/colorsync-utility/check-and-repair-color-profiles-csync003/mac](https://www.google.com/url?sa=E&q=https%3A%2F%2Fsupport.apple.com%2Fguide%2Fcolorsync-utility%2Fcheck-and-repair-color-profiles-csync003%2Fmac)dispwin - ArgyllCMS, [https://argyllcms.com/doc/dispwin.html](https://www.google.com/url?sa=E&q=https%3A%2F%2Fargyllcms.com%2Fdoc%2Fdispwin.html)(DaVinci Resolve) Question regarding transfer function ("gamma") for internet delivery : r/colorists - Reddit, [https://www.reddit.com/r/colorists/comments/1s96uip/davinci_resolve_question_regarding_transfer/](https://www.google.com/url?sa=E&q=https%3A%2F%2Fwww.reddit.com%2Fr%2Fcolorists%2Fcomments%2F1s96uip%2Fdavinci_resolve_question_regarding_transfer%2F)



结合新引入的资源，vcgt（Video Card Gamma Tag）标签的内部结构、Core Graphics 框架中的函数，以及它们与 RGB 一维查找表（1D LUTs）之间的关系有着极其严密的底层映射逻辑。

### 1. vcgt 标签的内容结构

vcgt 是 Apple 在 ColorSync 2.5 中引入的私有扩展标签，用于将设备的校准参数直接存储在 ICC 配置文件中 1。其内部结构包含一个**通用标头**和两种可选的**数据存储类型**：

**通用标头（Header）：**

- **0-3字节**：标签签名 vcgt（0x76636774） 2, 3。
- **4-7字节**：保留字段（设为 0） 3。
- **8-11字节**：类型签名，作为解析开关。frm 代表公式格式（Formula），tbl 代表表格格式（Table） 3。

**类型 A：公式结构（Formula-Based, frm ）**

- 用于可通过平滑的数学幂律曲线表示的伽马校正，参数极少，适合避免软件级量化伪影 4, 5。
- 为红、绿、蓝（RGB）每个通道分别存储 **Gamma、最小值（Min）和最大值（Max）** 4。
- 数值采用 u16Fixed16（32位无符号定点数）格式存储 4。系统利用公式 $f(x) = (Max - Min) \cdot x^{Gamma} + Min$ 将其计算为离散的一维查找表数据 5。

**类型 B：表格结构（Table-Based, tbl ）**

- 专业级显示器配置文件通常使用此格式，以支持任意非线性的灰阶或白点不规则校正 6。
- **12-13字节**：通道数（Channel Count），标准为 3（RGB） 7。
- **14-15字节**：条目数量（Entry Count），通常为 256 7。
- **16-17字节**：条目大小（Entry Size），专业配置几乎全为 2字节（16位）以保持高精度 7。
- **18字节及以后**：依次紧排红、绿、蓝三个通道的表格实际数据，这些数据是 0 到 65535 范围内的 16位整数 7。

### 2. Core Graphics 函数与 RGB 一维查找表的关系

Core Graphics（更确切地说是其子框架 Quartz Display Services）负责与显卡硬件进行实际通信 8。vcgt 数据与 CGGetDisplayTransferByTable / CGSetDisplayTransferByTable 的关系是**“持久化意图”与“运行时硬件状态”的序列化映射关系** 9。

- **CGSetDisplayTransferByTable（修改硬件状态）**：操作系统或第三方应用通过此函数将新的校准曲线应用到显示器硬件 10。
- **CGGetDisplayTransferByTable（检索硬件状态）**：用于直接从 Window Server 查询当前正在生效的显卡伽马斜坡状态 10, 11。开发者或校准软件通常调用它来验证之前的设置是否仍被激活 11。
- **RGB 三个一维查找表（1D LUTs）**：它们是 vcgt 数据在系统内存和显卡硬件中的**运行时表现形式** 12。在这个表中，索引（Index）代表应用输出的数字信号输入值，表中的值代表实际发送给显示器硬件的输出强度 12。

### 3. 数据转换与注入过程（从 vcgt 到 Core Graphics 查找表）

当 macOS 系统的 ColorSync Manager 加载显示器 ICC 文件时，这些组件按以下步骤联动：

- **字节序转换**：ICC 文件严格遵循大端序（Big-Endian），而现代 Mac 硬件是小端序，因此系统首先会对提取的 16位或32位整数进行字节交换 13。
- **归一化处理（从整数到浮点数）**：
- 对于 **表格类型 (tbl )**：系统将 vcgt 中 0-65535 的16位整数除以 $65535.0$。
- 对于 **公式类型 (frm )**：系统直接基于上述公式进行评估。
- 无论哪种格式，最终都会将二进制数据转化为 **3个完全独立的一维数组**（红、绿、蓝），数组的数据类型变为 CGGammaValue（在 Core Graphics 中表示一个 0.0, 1.0 范围内的 32位归一化浮点数） 11, 13。
- **硬件插值与写入**：系统将这三个归一化的一维浮点数组传递给 CGSetDisplayTransferByTable 13。此时，Core Graphics 会根据 GPU 硬件 LUT 的物理容量（例如现代 GPU 内部可能有 1024 个条目）执行线性插值，自动将 256 阶的数组平滑扩展填充至显卡的实际位深要求，从而使软件逻辑对底层显卡硬件细节保持透明 10。

简而言之，**vcgt 定义并静态存储了校准规范，ColorSync 负责解析并将其转化为 0.0-1.0 的浮点 RGB 一维数组，最后通过 CGSet... 函数将数组注入显卡，而 CGGet... 函数则是读取显卡当前正在使用的这三个数组的实时截面数据** 9-11, 13。

