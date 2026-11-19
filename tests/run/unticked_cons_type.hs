-- With DataKinds an unticked `:` in a type is the promoted cons (optics-core's
-- `Reverse (x : xs) acc = Reverse xs (x : acc)`).
{-# LANGUAGE DataKinds, TypeFamilies, PolyKinds, UndecidableInstances #-}
import Data.Proxy (Proxy (..))
import GHC.TypeLits (KnownNat, natVal, Nat)

type family Reverse (xs :: [k]) (acc :: [k]) :: [k] where
  Reverse '[] acc = acc
  Reverse (x : xs) acc = Reverse xs (x : acc)

type family Head (xs :: [Nat]) :: Nat where
  Head (x : _) = x

main :: IO ()
main = do
  print (natVal (Proxy :: Proxy (Head (Reverse '[1, 2, 3] '[]))))
  print (natVal (Proxy :: Proxy (Head (7 : '[]))))
