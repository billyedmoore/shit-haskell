-- The class and one method imported separately, the class exported with (..): the method that
-- is in scope goes out with it (resourcet re-exports MonadThrow this way).
module Re (Throw (..), Box (..)) where
import Cls (Throw, throwIt, Box, unBox)
