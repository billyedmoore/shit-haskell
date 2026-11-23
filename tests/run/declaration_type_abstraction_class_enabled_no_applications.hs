{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications #-}
import Data.Kind (Constraint, Type)
import Data.Proxy (Proxy)

type C :: forall k. k -> Constraint
class C @k a where
  m :: forall b. Proxy (a :: k) -> b -> b

main :: IO ()
main = pure ()
