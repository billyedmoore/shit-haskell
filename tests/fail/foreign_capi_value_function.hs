{-# LANGUAGE CApiFFI #-}

import Foreign.C.Types (CInt(..))

foreign import capi "stdlib.h value abs" c_abs :: CInt -> CInt

main = pure ()
