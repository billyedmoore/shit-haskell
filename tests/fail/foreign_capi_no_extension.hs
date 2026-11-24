{-# LANGUAGE NoCApiFFI #-}

import Foreign.C.Types (CInt(..))

foreign import capi "stdlib.h abs" c_abs :: CInt -> CInt

main = pure ()
