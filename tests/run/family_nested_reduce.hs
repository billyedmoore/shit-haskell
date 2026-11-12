{-# LANGUAGE DataKinds, TypeFamilies, PolyKinds, KindSignatures #-}
import Data.Proxy
import Data.Kind (Type)
type family IfM (b :: Maybe t) (c :: k) (d :: k) :: k
type instance IfM ('Just t) c d = c
type instance IfM 'Nothing c d = d
x :: IfM 'Nothing Maybe Proxy Int
x = Proxy
y :: IfM ('Just 3) Maybe Proxy Int
y = Just 1
main = print (x, y) >> main2
type family Or (a :: Maybe t) (b :: Maybe t) :: Maybe t where
  Or 'Nothing b = b
  Or a b = a
z :: IfM (Or 'Nothing 'Nothing) Maybe Proxy Int
z = Proxy
w :: IfM (Or 'Nothing ('Just 1)) Maybe Proxy Int
w = Just 2
main2 = print (z, w)
