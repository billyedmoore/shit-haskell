import Foreign.C.Types (CInt(..))

foreign import c_abs :: CInt -> CInt

main = pure ()
