{-# LANGUAGE PolyKinds, StandaloneKindSignatures, NoTypeAbstractions, TypeApplications #-}
import Data.Kind (Constraint, Type)
import Data.Proxy (Proxy)

type T :: forall k. k -> Type
data (T @k a) = T (Proxy (a :: k))

main :: IO ()
main = pure ()
