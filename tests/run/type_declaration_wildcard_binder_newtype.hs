{-# LANGUAGE StandaloneKindSignatures,TypeFamilies,PolyKinds,TypeOperators,TypeAbstractions #-}
import Data.Kind (Type)
newtype T _ = T Int
main :: IO ()
main = case (T 4 :: T Bool) of T n -> print n
