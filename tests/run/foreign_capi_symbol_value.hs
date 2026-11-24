{-# LANGUAGE CApiFFI #-}

import Foreign.C.Types (CInt(..))

foreign import capi "stdlib.h value" capiValue :: CInt -> CInt
foreign import ccall "value" cValue :: CInt -> CInt

main :: IO ()
main = pure ()
