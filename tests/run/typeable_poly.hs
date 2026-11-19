-- Typeable evidence under a `Typeable a` given: the constructor's part is built once, and the
-- fingerprints of applications (hashed from their parts' bytes, big-endian) match GHC's.
import Data.Typeable
import Foreign.Marshal.Alloc (allocaBytes)
import Foreign.Ptr (castPtr, Ptr)
import Foreign.Storable (peek, poke, peekByteOff)
import Data.Word (Word8)
import GHC.Fingerprint (Fingerprint (..), fingerprintFingerprints, fingerprintString)

wrap :: Typeable a => a -> TypeRep
wrap x = typeOf (Just [x])
{-# NOINLINE wrap #-}

count :: Typeable a => Int -> a -> Int
count 0 _ = 0
count n x = (if wrap x == typeOf (Just [True]) then 1 else 2) + count (n - 1) x

pair :: (Typeable a, Typeable b) => a -> b -> TypeRep
pair x y = typeOf (right (x, [y]))

right :: c -> Either () c
right = Right

bytes :: Fingerprint -> IO [Word8]
bytes fp = allocaBytes 16 $ \p -> do
  poke (castPtr p) fp
  back <- peek (castPtr p)
  print (back == fp)
  mapM (peekByteOff (p :: Ptr Word8)) [0 .. 15]

main :: IO ()
main = do
  print (count 3000 'c', count 3000 True)
  print (wrap (3 :: Int), pair 'x' (Just ()), pair [()] "s")
  print (typeRepFingerprint (typeOf (Just 'x')) == typeRepFingerprint (wrap 'x'))
  let fp = Fingerprint 0x0123456789abcdef 0xfedcba9876543210
  bytes fp >>= print
  print (fingerprintFingerprints [fp, fp], fingerprintString "hello")
