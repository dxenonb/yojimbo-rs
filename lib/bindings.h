#ifdef _MSC_VER
    /*
        The definition of NETCODE_CONST depends on how the library is compiled.

        This is generating different bindings on Windows than Ubuntu, and this
        causes NETCODE_CONST to always include `const` modifier.

        This feels incorrect and the wrong way to do this - PRs welcome! There
        may be flags we can pass to the `cc` crate in the build script.
    */
    #define __STDC__ 1
#endif

#include "netcode/netcode.h"
#include "reliable/reliable.h"