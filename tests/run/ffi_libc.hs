{-# LANGUAGE ForeignFunctionInterface #-}
import Foreign
import Foreign.C.Types
import Foreign.C.String

foreign import ccall unsafe "abs" c_abs :: CInt -> CInt
foreign import ccall unsafe "labs" c_labs :: CLong -> CLong
foreign import ccall unsafe "toupper" c_toupper :: CInt -> CInt
foreign import ccall unsafe "cos" c_cos :: CDouble -> CDouble
foreign import ccall unsafe "powf" c_powf :: CFloat -> CFloat -> CFloat
foreign import ccall unsafe "ldexp" c_ldexp :: CDouble -> CInt -> CDouble
foreign import ccall unsafe "atoi" c_atoi :: CString -> IO CInt
foreign import ccall unsafe "strchr" c_strchr :: CString -> CInt -> IO CString
foreign import ccall unsafe "llabs" c_llabs :: CLLong -> CLLong

main :: IO ()
main = do
  print (c_abs (-42), c_labs (-7), c_toupper 97, c_llabs (-123456789012))
  print (c_cos 0, c_powf 2 10, c_ldexp 3 4)
  n <- withCString "  1234xyz" c_atoi
  print n
  s <- withCString "hello world" $ \p -> do
    q <- c_strchr p (fromIntegral (fromEnum 'w'))
    peekCString q
  putStrLn s
  allocaBytes 16 $ \buf -> do
    pokeByteOff buf 0 (65 :: Word8)
    pokeByteOff buf 1 (0 :: Word8)
    peekCString (castPtr buf) >>= putStrLn
