{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, TypeFamilies #-}
import Data.Kind (Constraint, Type)
import Data.Proxy (Proxy)

type T :: forall k. k -> Type
type family T @k a

main :: IO ()
main = pure ()
