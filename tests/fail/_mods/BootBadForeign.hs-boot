{-# LANGUAGE ForeignFunctionInterface #-}
module BootBadForeign where
import Foreign.C.Types
foreign import ccall "abs" c_abs :: CInt -> IO CInt
