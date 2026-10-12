#pragma once

#ifdef emit
#pragma push_macro("emit")
#undef emit
#define KODOSI_RESTORE_QT_EMIT
#endif

#include <ghostty/vt.h>

#ifdef __cplusplus
inline void kodosi_ghostty_selection_emit(
    GhosttyTerminalSelectionFormatOptions* options,
    const GhosttyFormatterFormat format)
{
    options->emit = format;
}
#endif

#ifdef KODOSI_RESTORE_QT_EMIT
#pragma pop_macro("emit")
#undef KODOSI_RESTORE_QT_EMIT
#endif
