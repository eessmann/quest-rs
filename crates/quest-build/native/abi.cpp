#include <quest.h>
#if QUEST_VERSION_MAJOR != 4 || QUEST_VERSION_MINOR != 3
#error The supported native API is QuEST 4.3.x
#endif
#if QUEST_FLOAT_PRECISION != 2 || QUEST_INCLUDE_DEPRECATED_FUNCTIONS != 0
#error QuEST must use binary64 and disable deprecated APIs
#endif
static_assert(sizeof(qreal) == 8);
