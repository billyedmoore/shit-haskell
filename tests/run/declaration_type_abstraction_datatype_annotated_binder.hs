{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, TypeApplications #-}
import Data.Kind (Constraint, Type)
import Data.Proxy (Proxy)

type T :: forall k. k -> Type
data T @(k :: Type) a = T (Proxy (a :: k))

main :: IO ()
main = pure ()
