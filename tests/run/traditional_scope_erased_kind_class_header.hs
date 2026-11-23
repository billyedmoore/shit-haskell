{-# LANGUAGE PolyKinds, TypeApplications, TypeAbstractions, StandaloneKindSignatures #-}
import Data.Kind (Constraint)
import Data.Proxy (Proxy(..))

type C :: forall k. k -> Constraint
class C @k a where
  m :: forall b. Proxy (a :: k) -> b -> b
instance C Bool where
  m _ = id

main :: IO ()
main = print (m (Proxy :: Proxy Bool) (3 :: Int))
