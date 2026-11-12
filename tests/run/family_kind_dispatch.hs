{-# LANGUAGE DataKinds, TypeFamilies, PolyKinds, KindSignatures #-}
import Data.Kind (Type)
import Data.Proxy
data Coherence = INCOHERENT | COHERENT
data Sizing = Sized | Unsized
data Options (c :: Coherence) (s :: Sizing) (g :: Type) = Options
type family SetOptions (x :: k) (o :: Type) :: Type
type instance SetOptions (s :: Sizing) (Options c _s g) = Options c s g
type instance SetOptions (c :: Coherence) (Options _c s g) = Options c s g
type instance SetOptions (g :: Type) (Options c s _g) = Options c s g
class Show' a where show' :: Proxy a -> String
instance Show' (Options 'COHERENT 'Sized Int) where show' _ = "COHERENT Sized Int"
instance Show' (Options 'INCOHERENT 'Unsized Bool) where show' _ = "INCOHERENT Unsized Bool"
main = do
  putStrLn (show' (Proxy :: Proxy (SetOptions 'COHERENT (Options 'INCOHERENT 'Sized Int))))
  putStrLn (show' (Proxy :: Proxy (SetOptions Bool (SetOptions 'Unsized (Options 'INCOHERENT 'Sized Int)))))
