{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, TypeApplications #-}
import Data.Kind (Constraint, Type)
import Data.Proxy (Proxy)

type T :: forall k. k -> Type
data T @Type a = T

main :: IO ()
main = pure ()
