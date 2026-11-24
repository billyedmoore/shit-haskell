{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, RankNTypes #-}
import Data.Kind (Type)
type T :: forall k j. k -> j -> Type
data T @_ @j a b = T
main :: IO ()
main = pure ()
