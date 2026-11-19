-- With StarIsType, `*` is `Type` even between two types: reflection's
-- `mulProxy :: Proxy a -> Proxy b -> Proxy (a * b)` is `Proxy (a Type b)`, as GHC reads it.
{-# LANGUAGE KindSignatures, PolyKinds #-}
import Data.Proxy (Proxy (..))
import Data.Typeable (typeRep)

mulProxy :: Proxy a -> Proxy b -> Proxy (a * b)
mulProxy _ _ = Proxy

newtype Wrap (f :: * -> *) a = Wrap (f a)

main :: IO ()
main = do
  print (typeRep (mulProxy (Proxy :: Proxy Either) (Proxy :: Proxy ())))
  case Wrap (Just 'w') of Wrap m -> print m
