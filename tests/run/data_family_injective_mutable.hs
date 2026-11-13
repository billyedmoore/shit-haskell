-- vector's G.Vector instances: an injective family over-applied (Mutable v s a, Mutable having one
-- parameter) against a data instance's representation is compared as the family application.
{-# LANGUAGE TypeFamilies, MultiParamTypeClasses, FlexibleInstances, TypeFamilyDependencies #-}
import Control.Monad (liftM)
import Control.Monad.ST
import Data.Kind (Type)
import Data.STRef
import Data.Monoid (Any (..))

data family MV s a
newtype instance MV s Bool = MV_Bool (STRef s Bool)
newtype instance MV s Any = MV_Any (MV s Bool)

data family V a
newtype instance V Bool = V_Bool Bool
newtype instance V Any = V_Any (V Bool)

type family Mutable (v :: Type -> Type) = (mv :: Type -> Type -> Type) | mv -> v
type instance Mutable V = MV

class G v a where
  freeze :: Mutable v s a -> ST s (v a)
  thaw :: v a -> ST s (Mutable v s a)

instance G V Bool where
  freeze (MV_Bool r) = V_Bool `liftM` readSTRef r
  thaw (V_Bool b) = MV_Bool `liftM` newSTRef b

instance G V Any where
  freeze (MV_Any v) = V_Any `liftM` freeze v
  thaw (V_Any v) = MV_Any `liftM` thaw v

main :: IO ()
main = case runST (thaw (V_Any (V_Bool True)) >>= freeze) of V_Any (V_Bool b) -> print b
