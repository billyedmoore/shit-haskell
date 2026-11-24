{-# LANGUAGE PolyKinds, TypeAbstractions, NoTypeApplications, RankNTypes, StandaloneKindSignatures #-}
import Data.Kind (Type, Constraint)
import qualified Data.Kind as Kind
type T :: forall k. k -> Kind.Type
data T a @k = T
main :: IO ()
main = pure ()
